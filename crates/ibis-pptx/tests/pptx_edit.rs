// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenario tests of presentation editing (§2.4 of `tasks/doc/editing-plan.md`):
//! for each edit, run the commands, check the model, undo (back to the
//! start) and redo, save, read the saved file back (the same model), and
//! undo everything and save (the original file byte for byte).
//!
//! Saved files are written to `target/pptx-edit/` for the checks with the
//! real PowerPoint (`scripts/office-check/check.sh`).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use ibis_doc_model::{
    ArtisticEffect, AutoShape, Color, Command, Document, EmuRect, Fill, Geometry, LineEnd, EndSize, MediaFormat, MediaId, Picture, Paragraph, Presentation, Rgb, Run, RunStyle, Shape,
    ShapeId, ShapeKind, Slide, SlideId, Stroke, TextBody, TextStyle, ObjectDefaults, ShapeDefault,
};
use ibis_doc_model::model::HeaderFooterMarks;
use ibis_pptx::SaveOptions;

fn test_file(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test").join(name)
}

/// A model with the things a save and a read may number differently made
/// equal: ids (in the order met), media by their data, and a text box
/// as the rectangle a presentation stores it as.
fn normalize(p: &Presentation) -> Presentation {
    let mut p = p.clone();
    let media_key: HashMap<MediaId, u64> = p
        .media
        .iter()
        .map(|m| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            m.data.hash(&mut h);
            (m.id, h.finish())
        })
        .collect();
    let fix_media = |id: &mut MediaId| *id = MediaId(media_key.get(id).copied().unwrap_or(0));
    // Every kind of picture fill (stretched, tiled, with the adjustments of a:blipFill) is compared by its picture's data.
    let fix_fill = |fill: &mut Fill| match fill {
        Fill::Image(m) | Fill::TileImage { media: m, .. } => fix_media(m),
        Fill::Blip(b) => fix_media(&mut b.media),
        _ => {}
    };
    // Links to slides by the slides' places (one to a slide not there is none).
    let places: HashMap<SlideId, usize> = p.slides.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
    let fix_link = |l: &mut Option<ibis_doc_model::SlideLink>| {
        if let Some(ibis_doc_model::SlideLink::Slide(id)) = l {
            *l = places.get(id).map(|i| ibis_doc_model::SlideLink::Slide(SlideId(*i as u64)));
        }
    };
    let fix_links = |body: &mut TextBody| {
        for run in body.paragraphs.iter_mut().flat_map(|p| p.runs.iter_mut()) {
            fix_link(&mut run.style.link);
        }
    };
    for slide in &mut p.slides {
        for shape in &mut slide.shapes {
            if let Some(b) = shape.text_body_mut() {
                fix_links(b);
            }
            if let ShapeKind::Table(t) = &mut shape.kind {
                for c in t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()) {
                    fix_links(&mut c.text);
                }
            }
        }
        let links: Vec<(ShapeId, Option<ibis_doc_model::SlideLink>)> = slide.shape_links.iter().map(|(k, v)| {
            let mut l = Some(v.clone());
            fix_link(&mut l);
            (*k, l)
        }).collect();
        slide.shape_links = links.into_iter().filter_map(|(k, v)| Some((k, v?))).collect();
    }
    let fix_shape = |s: &mut Shape| {
        s.id = ShapeId(0);
        if let ShapeKind::TextBox(body) = &s.kind {
            s.kind = ShapeKind::Auto(AutoShape::text_box(Fill::None, body.clone()));
        }
        match &mut s.kind {
            ShapeKind::Picture(pic) => {
                fix_media(&mut pic.media);
                if let Some(clip) = &mut pic.clip {
                    fix_media(&mut clip.media);
                }
                pic.name.clear();
            }
            ShapeKind::Auto(a) => {
                fix_fill(&mut a.fill);
            }
            _ => {}
        }
        // A table's cells' text, the same way.
        if let ShapeKind::Table(t) = &mut s.kind {
            for cell in t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()) {
                resolve_text(&mut cell.text);
            }
            // Where rows and columns came from is the reader's to say.
            t.column_sources.clear();
            for row in &mut t.rows {
                row.source = None;
            }
        }
        if let Some(body) = s.text_body_mut() {
            resolve_text(body);
            for para in &mut body.paragraphs {
                if let Some(b) = &mut para.bullet
                    && let Some(m) = &mut b.picture
                {
                    fix_media(m);
                }
            }
        }
    };
    for slide in &mut p.slides {
        // Shapes over a chart are stored relative to it: their frames read
        // back within a unit or two (see `check_saved`).
        let attached: Vec<ShapeId> = slide.attached.values().flatten().copied().collect();
        for shape in slide.shapes.iter_mut().filter(|sh| attached.contains(&sh.id)) {
            shape.frame = EmuRect::default();
        }
        // Attached shapes by their places on the slide.
        let place = |id: &ShapeId| ShapeId(slide.shape_index(*id).map_or(u64::MAX, |i| i as u64));
        fn group_by_place(group:&ibis_doc_model::ShapeGroup,slide:&Slide)->Option<ibis_doc_model::ShapeGroup> {
            use ibis_doc_model::GroupMember;
            let mut out=group.clone();out.children=group.children.iter().filter_map(|child|match child {GroupMember::Shape(id)=>slide.shape_index(*id).map(|index|GroupMember::Shape(ShapeId(index as u64))),GroupMember::Group(inner)=>group_by_place(inner,slide).map(|inner|GroupMember::Group(Box::new(inner)))}).collect();
            if !group.children.is_empty() && out.children.is_empty() && !group.has_unmodelled_children {return None;}Some(out)
        }
        let groups_by_place=slide.groups.iter().filter_map(|group|group_by_place(group,slide)).collect();
        let attached_by_place = slide
            .attached
            .iter()
            .filter(|(chart, _)| slide.shape(**chart).is_some())
            .map(|(chart, ids)| (place(chart), ids.iter().filter(|a| slide.shape(**a).is_some()).map(place).collect()))
            .collect();
        let placeholders_by_place = slide.placeholders.iter().filter(|(id, _)| slide.shape(**id).is_some()).map(|(id, p)| (place(id), p.clone())).collect();
        // Hidden shapes by place; names are compared in `check_saved` (a new
        // shape's is the writer's).
        let hidden_by_place = slide.hidden_shapes.iter().filter(|id| slide.shape(**id).is_some()).map(place).collect();
        let links_by_place = slide.shape_links.iter().filter(|(id, _)| slide.shape(**id).is_some()).map(|(id, l)| (place(id), l.clone())).collect();
        let link_tooltips_by_place = slide.shape_link_tooltips.iter().filter(|(id, _)| slide.shape(**id).is_some()).map(|(id, t)| (place(id), t.clone())).collect();
        let locked_by_place = slide.aspect_locked.iter().filter(|id| slide.shape(**id).is_some()).map(place).collect();
        let alt_texts_by_place = slide.shape_alt_texts.iter().filter(|(id, _)| slide.shape(**id).is_some()).map(|(id, t)| (place(id), t.clone())).collect();
        let return_to_zoom_by_place = slide.shape_return_to_zoom.iter().filter(|id| slide.shape(**id).is_some()).map(place).collect();
        let zoom_show_bg_by_place = slide.shape_zoom_show_bg.iter().filter(|(id, _)| slide.shape(**id).is_some()).map(|(id, v)| (place(id), *v)).collect();
        let zoom_durations_by_place = slide.shape_zoom_durations.iter().filter(|(id, _)| slide.shape(**id).is_some()).map(|(id, v)| (place(id), *v)).collect();
        let glues_by_place = slide
            .shape_glues
            .iter()
            .filter(|(id, _)| slide.shape(**id).is_some())
            .map(|(id, glue)| {
                let normalize_end = |end: Option<(u64, u32)>| -> Option<(u64, u32)> {
                    let (target_id, site) = end?;
                    let target_index = slide.shape_index(ShapeId(target_id))?;
                    Some((target_index as u64, site))
                };
                let norm_glue = ibis_doc_model::Glue {
                    start: normalize_end(glue.start),
                    end: normalize_end(glue.end),
                };
                (place(id), norm_glue)
            })
            .collect();
        slide.hidden_shapes = hidden_by_place;
        slide.shape_links = links_by_place;
        slide.shape_link_tooltips = link_tooltips_by_place;
        slide.aspect_locked = locked_by_place;
        slide.shape_alt_texts = alt_texts_by_place;
        slide.shape_return_to_zoom = return_to_zoom_by_place;
        slide.shape_zoom_show_bg = zoom_show_bg_by_place;
        slide.shape_zoom_durations = zoom_durations_by_place;
        slide.shape_glues = glues_by_place;
        slide.shape_names.clear();
        slide.attached = attached_by_place;
        slide.groups = groups_by_place;
        slide.placeholders = placeholders_by_place;
        slide.id = SlideId(0);
        slide.layout = None;
        fix_fill(&mut slide.background);
        slide.shapes.iter_mut().for_each(fix_shape);
        slide.backdrop.iter_mut().for_each(fix_shape);
    }
    for master in &mut p.masters {
        master.id = ibis_doc_model::MasterId(0);
    }
    for layout in &mut p.layouts {
        layout.id = ibis_doc_model::LayoutId(0);
        // (A layout pasted in has no master in the model until the file is read again.)
        layout.master = None;
        layout.backdrop.iter_mut().for_each(fix_shape);
        fix_fill(&mut layout.background);
    }
    // Pictures are compared by their data (above); a file keeps only the
    // media its slides use.
    p.media.clear();
    p
}

/// A text body with every run's format written out, as the reader gives
/// it, runs of the same format joined (the reader splits runs where the
/// script changes) and fonts left out (a new run with no font reads back
/// with the theme's).
fn resolve_text(body: &mut TextBody) {
    let base = body.style.clone();
    let resolve = |run: &RunStyle| {
        let s = base.with_run(run);
        RunStyle { font_size: Some(s.font_size), bold: Some(s.bold), italic: Some(s.italic), color: Some(s.color), font: None, fallback_font: None, lang: None, ..run.clone() }
    };
    for p in &mut body.paragraphs {
        p.align = Some(p.align.unwrap_or(base.align));
        p.end_style = resolve(&p.end_style);
        let mut runs: Vec<Run> = Vec::new();
        for run in p.runs.drain(..) {
            let style = resolve(&run.style);
            match runs.last_mut() {
                Some(last) if last.style == style => last.text.push_str(&run.text),
                _ => runs.push(Run { text: run.text, style }),
            }
        }
        p.runs = runs;
    }
    let first = body.paragraphs.iter().find_map(|p| p.runs.first().map(|r| (p.align, r.style.clone())));
    let (align, run) = first.unwrap_or_else(|| (body.paragraphs.first().and_then(|p| p.align), body.paragraphs.first().map(|p| p.end_style.clone()).unwrap_or_default()));
    body.style = TextStyle { align: align.unwrap_or_default(), ..TextStyle::default() }.with_run(&run);
}

/// A text body, one line per paragraph and run, for failure messages.
fn describe(body: &TextBody) -> String {
    let mut out = format!("  body {:?}\n", body.style);
    for p in &body.paragraphs {
        out += &format!("  ¶ {:?}\n", Paragraph { runs: Vec::new(), ..p.clone() });
        for r in &p.runs {
            out += &format!("    {:?} {:?}\n", r.text, r.style);
        }
    }
    out
}

struct Scenario {
    name: String,
    data: Vec<u8>,
    opened: ibis_pptx::Opened,
    document: Document,
}

impl Scenario {
    fn open(name: &'static str, file: &str) -> Self {
        let data = std::fs::read(test_file(file)).unwrap();
        let opened = ibis_pptx::open(data.clone()).unwrap();
        let document = Document::new(opened.presentation.clone());
        let stem = std::path::Path::new(file).file_stem().and_then(|s| s.to_str()).unwrap_or("");
        Self { name: format!("{stem}-{name}"), data, opened, document }
    }

    fn p(&self) -> &Presentation {
        self.document.presentation()
    }

    /// Run `commands` as one step and check undo, redo, save and read back.
    /// Returns the saved file.
    fn run(&mut self, commands: Vec<Command>) -> Vec<u8> {
        let before = self.p().clone();
        self.document.execute(commands).unwrap();
        let after = self.p().clone();
        assert_ne!(before, after, "{}: the edit changed nothing", self.name);
        assert!(self.document.undo());
        assert_eq!(self.p(), &before, "{}: undo", self.name);
        assert!(self.document.redo());
        assert_eq!(self.p(), &after, "{}: redo", self.name);
        self.check_saved()
    }

    /// Save, read back and compare; save again with everything undone.
    fn check_saved(&mut self) -> Vec<u8> {
        let saved = self.opened.source.save(self.p(), SaveOptions::default()).unwrap_or_else(|e| panic!("{}: {e}", self.name));
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/pptx-edit");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{}.pptx", self.name)), &saved).unwrap();
        let reread = ibis_pptx::read(&saved).unwrap_or_else(|e| panic!("{}: saved file does not read: {e}", self.name));
        let section_indices=|p:&Presentation|p.sections.iter().map(|section|(section.id.clone(),section.name.clone(),section.slides.iter().map(|id|p.slide_index(*id)).collect::<Vec<_>>())).collect::<Vec<_>>();
        assert_eq!(section_indices(&reread),section_indices(self.p()),"{}: section identity/name/membership after saving",self.name);
        // Shapes over charts: within 2 EMU.
        for (x, y) in self.p().slides.iter().zip(&reread.slides) {
            let frames = |sl: &Slide| -> Vec<EmuRect> { sl.shapes.iter().filter(|sh| sl.is_attached(sh.id)).map(|sh| sh.frame).collect() };
            for (f, g) in frames(x).iter().zip(frames(y)) {
                let close = [(f.x, g.x), (f.y, g.y), (f.width, g.width), (f.height, g.height)].iter().all(|(u, v)| (u - v).abs() <= 2);
                assert!(close, "{}: a shape over a chart reads back at {g:?}, not {f:?}", self.name);
            }
        }
        // The model's names read back (by place).
        for (x, y) in self.p().slides.iter().zip(&reread.slides) {
            for (j, shape) in x.shapes.iter().enumerate() {
                if let (Some(name), Some(other)) = (x.shape_names.get(&shape.id), y.shapes.get(j)) {
                    assert_eq!(y.shape_names.get(&other.id), Some(name), "{}: the name of shape {j}", self.name);
                }
            }
        }
        let (a, b) = (normalize(self.p()), normalize(&reread));
        assert_eq!(a.slides.len(), b.slides.len(), "{}: slide count", self.name);
        for (i, (x, y)) in a.slides.iter().zip(&b.slides).enumerate() {
            assert_eq!(x.shapes.len(), y.shapes.len(), "{}: slide {} shape count", self.name, i + 1);
            for (j, (s, t)) in x.shapes.iter().zip(&y.shapes).enumerate() {
                if s != t {
                    if let (Some(a), Some(b)) = (s.text_body(), t.text_body()) {
                        eprintln!("model:\n{}\nread back:\n{}", describe(a), describe(b));
                    }
                }
                assert_eq!(s, t, "{}: slide {} shape {} reads back differently", self.name, i + 1, j);
            }
            for (j, (s, t)) in x.backdrop.iter().zip(&y.backdrop).enumerate() {
                assert_eq!(s, t, "{}: slide {} backdrop shape {} reads back differently", self.name, i + 1, j);
            }
            assert_eq!(x, y, "{}: slide {} reads back differently", self.name, i + 1);
        }
        assert_eq!(a.layouts, b.layouts, "{}: layouts read back differently", self.name);
        // (Not the whole model: the next free id is the reader's own.)
        assert!(a.slide_size == b.slide_size && a.fonts == b.fonts, "{}: reads back differently", self.name);
        // Saved twice, the same file.
        let again = self.opened.source.save(self.p(), SaveOptions::default()).unwrap();
        assert!(again == saved, "{}: saving is not repeatable", self.name);
        // Everything undone, the original file.
        let mut undone = self.document.clone();
        while undone.undo() {}
        let original = self.opened.source.save(undone.presentation(), SaveOptions::default()).unwrap();
        assert!(original == self.data, "{}: undoing everything does not give the original file", self.name);
        saved
    }
}

/// The slides' ids and the shapes with frames on the first slide that has some.
fn first_shapes(p: &Presentation) -> (SlideId, Vec<Shape>) {
    let slide = p.slides.iter().find(|s| s.shapes.len() >= 2).expect("a slide with shapes");
    (slide.id, slide.shapes.clone())
}

#[test]
fn move_resize_rotate_and_flip_shapes() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("move", file);
        let (slide, shapes) = first_shapes(s.p());
        let mut commands = Vec::new();
        for shape in &shapes {
            let f = shape.frame;
            commands.push(Command::SetFrame { slide, shape: shape.id, frame: EmuRect::new(f.x + 127_000, f.y + 254_000, f.width + 12_700, f.height.max(1) * 2) });
            commands.push(Command::SetTransform { slide, shape: shape.id, rotation: 1_800_000, flip_h: !shape.flip_h, flip_v: false });
        }
        s.run(commands);
    }
}

#[test]
fn every_slide_shape_moves() {
    // Every shape of every slide, including grouped ones, placeholders and charts.
    let mut s = Scenario::open("move-all", "benchmark/earnings-presentation.pptx");
    let targets: Vec<(SlideId, ShapeId, EmuRect)> =
        s.p().slides.iter().flat_map(|sl| sl.shapes.iter().filter(|sh| !sl.is_attached(sh.id)).map(move |sh| (sl.id, sh.id, sh.frame))).collect();
    // Charts move with the shapes drawn over them.
    for (slide, shape, frame) in targets {
        s.document.execute(vec![Command::SetFrame { slide, shape, frame: frame.translated(12_700, 25_400) }]).unwrap();
        if let Err(e) = s.opened.source.save(s.p(), SaveOptions::default()) {
            panic!("{e}");
        }
    }
    s.check_saved();
}

#[test]
fn remove_shapes_and_change_z_order() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("remove", file);
        let (slide, shapes) = first_shapes(s.p());
        s.run(vec![Command::RemoveShape { slide, shape: shapes[0].id }]);
        let mut s = Scenario::open("z-order", file);
        s.run(vec![Command::MoveShape { slide, shape: shapes[0].id, index: shapes.len() - 1 }]);
        let mut s = Scenario::open("z-order-back", file);
        s.run(vec![Command::MoveShape { slide, shape: shapes[shapes.len() - 1].id, index: 0 }]);
    }
}

#[test]
fn remove_every_shape_of_every_slide() {
    let mut s = Scenario::open("remove-all", "benchmark/earnings-presentation.pptx");
    let mut commands = Vec::new();
    for slide in &s.p().slides {
        // A chart takes the shapes drawn over it along.
        for shape in slide.shapes.iter().filter(|sh| !slide.is_attached(sh.id)) {
            commands.push(Command::RemoveShape { slide: slide.id, shape: shape.id });
        }
    }
    // Shapes the reader made (over charts) cannot be removed on their own.
    match s.document.execute(commands) {
        Ok(()) => {
            if let Err(e) = s.opened.source.save(s.p(), SaveOptions::default()) {
                assert!(e.to_string().contains("まだ保存できません"), "{e}");
            }
        }
        Err(e) => panic!("{e:?}"),
    }
}

/// A tiny PNG: `w` × `h` pixels of one colour.
fn png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xffff_ffffu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend(data);
        out.extend(&body);
        out.extend(crc(&body).to_be_bytes());
    }
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::new();
    ihdr.extend(w.to_be_bytes());
    ihdr.extend(h.to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    let mut raw = Vec::new();
    for _ in 0..h {
        raw.push(0);
        for _ in 0..w {
            raw.extend(rgb);
        }
    }
    chunk(&mut out, b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6));
    chunk(&mut out, b"IEND", &[]);
    out
}

fn new_shapes(p: &mut Presentation) -> Vec<Shape> {
    let body = TextBody::plain("新しいテキスト ボックス\n二行目", TextStyle { font_size: 2400, bold: true, color: Rgb(0x1f4e79), ..TextStyle::default() });
    let text_box = Shape::new(p.new_shape_id(), EmuRect::new(914_400, 914_400, 3_657_600, 914_400), ShapeKind::TextBox(body));
    let mut rect = AutoShape::new(Geometry::Rect, Fill::Solid(Color::opaque(Rgb(0x4472c4))), Some(Stroke::solid(Color::opaque(Rgb(0x2f528f)), 12_700)));
    rect.text = Some(TextBody::plain("四角形", TextStyle::default()));
    let rect = Shape::new(p.new_shape_id(), EmuRect::new(1_828_800, 2_743_200, 1_828_800, 914_400), ShapeKind::Auto(rect));
    let oval = Shape::new(
        p.new_shape_id(),
        EmuRect::new(4_572_000, 2_743_200, 1_371_600, 1_371_600),
        ShapeKind::Auto(AutoShape::new(Geometry::Ellipse, Fill::Solid(Color { rgb: Rgb(0xed7d31), alpha: 128 }), None)),
    );
    let round = Shape::new(
        p.new_shape_id(),
        EmuRect::new(6_400_800, 2_743_200, 1_828_800, 914_400),
        ShapeKind::Auto(AutoShape::new(Geometry::RoundRect { radius: 25_000 }, Fill::None, Some(Stroke::solid(Color::BLACK, 25_400).with_dash(ibis_doc_model::LineDash::Dash)))),
    );
    let mut arrow = Stroke::solid(Color::opaque(Rgb(0xc00000)), 19_050);
    arrow.tail = LineEnd::Triangle(EndSize { width: 1, length: 1 });
    let mut line = Shape::new(p.new_shape_id(), EmuRect::new(914_400, 4_572_000, 2_743_200, 457_200), ShapeKind::Line(arrow));
    line.flip_v = true;
    let media = p.add_media(MediaFormat::Png, png(4, 3, [0x33, 0x99, 0x66]));
    let picture = Shape::new(
        p.new_shape_id(),
        EmuRect::new(6_400_800, 4_572_000, 1_219_200, 914_400),
        ShapeKind::Picture(Picture { media, crop: Default::default(), stroke: None, corner: 0, duotone: None, clear: None, luminance: None, clip: None, shadow: None, reflection: None, soft_edge: None, alt_text: Default::default(), decorative: false, name: String::new(), geometry: None, effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None }),
    );
    vec![text_box, rect, oval, round, line, picture]
}

#[test]
fn insert_shapes() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("insert", file);
        let slide = s.p().slides[0].id;
        let count = s.p().slides[0].shapes.len();
        let shapes = new_shapes(s.document.ids());
        let commands = shapes.into_iter().enumerate().map(|(i, shape)| Command::InsertShape { slide, index: count + i, shape }).collect();
        let saved = s.run(commands);
        // New text refers to the theme's body fonts (`+mn-lt`, `+mn-ea`),
        // which read back as the theme's typefaces.
        let reread = ibis_pptx::read(&saved).unwrap();
        let theme = reread.theme_fonts.clone().expect("theme fonts").minor;
        let run = &reread.slides[0].shapes[count].text_body().unwrap().paragraphs[0].runs[0];
        assert_eq!(run.style.font.as_deref(), theme.for_text(&run.text), "{}", s.name);
        // At the back too.
        let mut s = Scenario::open("insert-back", file);
        let slide = s.p().slides[1].id;
        let shape = new_shapes(s.document.ids()).remove(1);
        s.run(vec![Command::InsertShape { slide, index: 0, shape }]);
    }
}

#[test]
fn fill_and_outline() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("fill", file);
        let (slide, shape) = s
            .p()
            .slides
            .iter()
            .find_map(|sl| sl.shapes.iter().find(|sh| matches!(sh.kind, ShapeKind::Auto(_))).map(|sh| (sl.id, sh.id)))
            .expect("an auto shape");
        let arrow = Stroke::solid(Color::opaque(Rgb(0x70ad47)), 38_100).with_dash(ibis_doc_model::LineDash::LgDashDot);
        s.run(vec![
            Command::SetFill { slide, shape, fill: Fill::Solid(Color::opaque(Rgb(0xffc000))) },
            Command::SetStroke { slide, shape, stroke: Some(arrow) },
        ]);
        s.run(vec![Command::SetStroke { slide, shape, stroke: None }, Command::SetFill { slide, shape, fill: Fill::None }]);
    }
}

#[test]
fn reorder_remove_and_add_slides() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("slide-move", file);
        let ids: Vec<SlideId> = s.p().slides.iter().map(|s| s.id).collect();
        s.run(vec![Command::MoveSlide { slide: ids[0], index: ids.len() - 1 }]);

        let mut s = Scenario::open("slide-remove", file);
        s.run(vec![Command::RemoveSlide { slide: ids[1] }]);

        let mut s = Scenario::open("slide-new", file);
        let layout = s.p().layouts[1.min(s.p().layouts.len() - 1)].clone();
        let id = s.document.ids().new_slide_id();
        s.run(vec![Command::InsertSlide { index: 1, slide: Slide::from_layout(id, &layout) }]);

        let mut s = Scenario::open("slide-duplicate", file);
        let id = s.document.ids().new_slide_id();
        let copy = Slide { id, ..s.p().slides[0].clone() };
        s.run(vec![Command::InsertSlide { index: 1, slide: copy }]);
        // The copy edits on its own.
        let slide = s.p().slides[1].clone();
        if let Some(shape) = slide.shapes.first() {
            s.run(vec![Command::SetFrame { slide: id, shape: shape.id, frame: shape.frame.translated(254_000, 0) }]);
        }
    }
}

/// 新しいスライド with its layout's placeholders (Source::new_slide): they
/// read back as they are, take typed text, and one can be deleted.
#[test]
fn new_slides_with_placeholders() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("slide-new-placeholders", file);
        let layout = s.p().layouts[1.min(s.p().layouts.len() - 1)].id;
        let id = s.document.ids().new_slide_id();
        let current = s.p().clone();
        let slide = {
            let Scenario { opened, document, .. } = &mut s;
            opened.source.new_slide(&current, 1, id, layout, || document.ids().new_shape_id()).unwrap()
        };
        assert!(!slide.shapes.is_empty(), "{}: the layout's placeholders", s.name);
        assert_eq!(slide.shapes.len(), slide.placeholders.len(), "{}: all placeholders", s.name);
        assert!(slide.shapes.iter().all(|sh| sh.text_body().is_none_or(|b| b.is_empty())), "{}: empty", s.name);
        s.run(vec![Command::InsertSlide { index: 1, slide: slide.clone() }]);
        let typed = slide.shapes.iter().find(|sh| sh.text_body().is_some()).expect("a placeholder with text").id;
        s.run(vec![Command::EditText { slide: id, shape: typed, at: 0, remove: 0, insert: "新しい見出し".into() }]);
        assert_eq!(s.p().slide(id).unwrap().shape(typed).unwrap().text_body().unwrap().text(), "新しい見出し");
        let last = slide.shapes.last().unwrap().id;
        s.run(vec![Command::RemoveShapes { slide: id, shapes: vec![last] }]);
    }
}

/// コメント: added (on the slide and on a shape), answered, resolved,
/// removed; written as threaded comments, read back the same. A slide with
/// older comments refuses the change.
#[test]
fn comments_added_answered_resolved_and_removed() {
    use ibis_doc_model::{Author, CommentReply, EmuPoint, SlideComment};
    let me = Author { name: "試験 太郎".into(), initials: "試".into() };
    let mut s = Scenario::open("comments", "editing/pptx-comments-modern.pptx");
    let (slide, title) = (s.p().slides[0].id, s.p().slides[0].shapes[1].id);
    let mut comments = s.p().slides[0].comments.clone();
    // An answer to コメント1, and コメント2 opened again.
    comments[0].replies.push(CommentReply { id: "{E0000000-0000-4000-8000-000000000001}".into(), author: me.clone(), created: "2026-10-07T02:00:00".into(), text: "返信2".into() });
    comments[1].resolved = false;
    s.run(vec![Command::SetComments { slide, comments: comments.clone() }]);
    // New ones: on the slide, and on the subtitle.
    comments.push(SlideComment { id: "{E0000000-0000-4000-8000-000000000002}".into(), author: me.clone(), created: "2026-10-07T02:01:00".into(), text: "新しい\nコメント".into(), position: Some(EmuPoint::new(0, 0)), shape: None, resolved: false, replies: Vec::new() });
    comments.push(SlideComment { id: "{E0000000-0000-4000-8000-000000000003}".into(), author: me.clone(), created: "2026-10-07T02:02:00".into(), text: "サブタイトルへ".into(), position: None, shape: Some(title), resolved: true, replies: Vec::new() });
    s.run(vec![Command::SetComments { slide, comments: comments.clone() }]);
    // The second slide's only comment removed: its part goes.
    let second = s.p().slides[1].id;
    let saved = s.run(vec![Command::SetComments { slide: second, comments: Vec::new() }]);
    let names: Vec<String> = ibis_ooxml::Package::open(saved).unwrap().names().map(str::to_string).collect();
    assert_eq!(names.iter().filter(|n| n.starts_with("ppt/comments/")).count(), 1, "{names:?}");
    // A first comment on a slide that had none.
    let mut s = Scenario::open("comments-new", "editing/pptx-plan.pptx");
    let slide = s.p().slides[1].id;
    s.run(vec![Command::SetComments { slide, comments: vec![SlideComment { id: "{E0000000-0000-4000-8000-000000000004}".into(), author: me.clone(), created: "2026-10-07T02:03:00".into(), text: "初めての".into(), position: Some(EmuPoint::new(0, 0)), shape: None, resolved: false, replies: Vec::new() }] }]);
    // Older comments: not rewritten.
    let s = Scenario::open("comments-legacy", "editing/pptx-comments-legacy.pptx");
    let mut p = s.p().clone();
    p.slides[0].comments[0].resolved = true;
    assert!(s.opened.source.save(&p, SaveOptions::default()).is_err());
}

/// グラフのデータの編集: a value, a series' name and a text category
/// changed (and a point emptied), written into the chart's caches, read
/// back the same; a change of anything else is refused.
#[test]
fn chart_data_edited() {
    let mut s = Scenario::open("chart-data", "benchmark/earnings-presentation.pptx");
    let (slide, shape, mut chart) = s
        .p()
        .slides
        .iter()
        .find_map(|sl| sl.shapes.iter().find_map(|sh| match &sh.kind {
            ShapeKind::Chart(c) if c.groups.iter().any(|g| g.series.iter().any(|x| x.values.len() > 2 && !x.categories.is_empty())) => Some((sl.id, sh.id, (**c).clone())),
            _ => None,
        }))
        .expect("a chart");
    let series = chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()).find(|x| x.values.len() > 2 && !x.categories.is_empty()).unwrap();
    series.values[0] = Some(ibis_doc_model::Number(12345.5));
    series.values[1] = None;
    series.name = "書き換えた系列".into();
    let text_categories = !series.categories[0].chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',');
    if text_categories {
        series.categories[0] = "新しい項目".into();
        // Every series shows the same categories.
        let cats = series.categories.clone();
        for g in chart.groups.iter_mut() {
            for x in g.series.iter_mut().filter(|x| x.categories.len() == cats.len()) {
                x.categories = cats.clone();
            }
        }
    }
    s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(Box::new(chart.clone()))) }]);
    // Not data: refused.
    let mut other = chart.clone();
    other.background = ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(Rgb(0x123456)));
    let mut p = s.p().clone();
    p.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(other));
    assert!(s.opened.source.save(&p, SaveOptions::default()).is_err());
}

/// グラフタイトル: a chart's title hidden, and shown again, read back the same.
#[test]
fn chart_title_hidden_and_shown() {
    use ibis_doc_model::ChartTitle;
    let mut s = Scenario::open("chart-title", "benchmark/earnings-presentation.pptx");
    let (slide, shape, chart) = s
        .p()
        .slides
        .iter()
        .find_map(|sl| sl.shapes.iter().find_map(|sh| match &sh.kind {
            ShapeKind::Chart(c) if c.title.is_some() => Some((sl.id, sh.id, (**c).clone())),
            _ => None,
        }))
        .expect("a chart with a title");
    let original_title = chart.title.clone().unwrap();

    let (slide_idx, shape_idx) = s.p().slides.iter().enumerate().find_map(|(si, sl)| sl.shapes.iter().position(|sh| sh.id == shape).map(|shi| (si, shi))).unwrap();

    let set = |s: &mut Scenario, chart: ibis_doc_model::Chart| s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(Box::new(chart))) }]);

    // 1. 既存タイトルの非表示（削除）
    let mut hidden = chart.clone();
    hidden.title = None;
    let saved_hidden = set(&mut s, hidden.clone());
    let reloaded = ibis_pptx::read(&saved_hidden).unwrap();
    let ShapeKind::Chart(rc) = &reloaded.slides[slide_idx].shapes[shape_idx].kind else { panic!("a chart") };
    assert!(rc.title.is_none(), "title is hidden after save and reload");

    // 2. 既存タイトルの再表示（テキスト変更）
    let mut shown = hidden.clone();
    shown.title = Some(ChartTitle { text: "新タイトル".into(), ..original_title.clone() });
    let saved_shown = set(&mut s, shown);
    let reloaded2 = ibis_pptx::read(&saved_shown).unwrap();
    let ShapeKind::Chart(rc2) = &reloaded2.slides[slide_idx].shapes[shape_idx].kind else { panic!("a chart") };
    assert_eq!(rc2.title.as_ref().map(|t| t.text.as_str()), Some("新タイトル"), "title is restored after save and reload");

    // 3. タイトルのない新規チャートへのタイトル追加（None -> Some 新設パスの検証）
    let mut s2 = Scenario::open("chart-title-new", "editing/pptx-plan.pptx");
    let slide2 = s2.p().slides[1].id;
    let id2 = s2.document.ids().new_shape_id();
    let current2 = s2.p().clone();
    let new_chart = s2.opened.source.new_chart(&current2, slide2, id2, EmuRect::new(0, 0, 6_000_000, 4_000_000)).unwrap();
    let index2 = s2.p().slides[1].shapes.len();
    s2.run(vec![Command::InsertShape { slide: slide2, index: index2, shape: new_chart.clone() }]);
    let ShapeKind::Chart(c) = &new_chart.kind else { panic!("a chart") };
    let mut with_title = (**c).clone();
    assert!(with_title.title.is_none(), "new chart starts without title");
    with_title.title = Some(ChartTitle { text: "追加されたタイトル".into(), style: ibis_doc_model::ChartText { size: 1400, ..(**c).axes[0].text.clone() }, at: None });
    let saved_new = s2.run(vec![Command::SetKind { slide: slide2, shape: id2, kind: Box::new(ShapeKind::Chart(Box::new(with_title))) }]);
    let reloaded3 = ibis_pptx::read(&saved_new).unwrap();
    let ShapeKind::Chart(rc3) = &reloaded3.slides[1].shapes.last().unwrap().kind else { panic!("a chart") };
    assert_eq!(rc3.title.as_ref().map(|t| t.text.as_str()), Some("追加されたタイトル"), "new title is saved and reloaded");
}

/// 凡例: a chart's legend moved to another side, hidden, and shown again
/// (as the file reads the new one), read back the same.
#[test]
fn chart_legend_moved_hidden_and_shown() {
    use ibis_doc_model::{Legend, LegendPosition, Side};
    let mut s = Scenario::open("chart-legend", "benchmark/earnings-presentation.pptx");
    let (slide, shape, chart) = s
        .p()
        .slides
        .iter()
        .find_map(|sl| sl.shapes.iter().find_map(|sh| match &sh.kind {
            ShapeKind::Chart(c) if c.legend.is_some() => Some((sl.id, sh.id, (**c).clone())),
            _ => None,
        }))
        .expect("a chart with a legend");
    let set = |s: &mut Scenario, chart: ibis_doc_model::Chart| s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(Box::new(chart))) }]);
    let mut moved = chart.clone();
    let legend = moved.legend.take().unwrap();
    let side = if legend.source_position == Some(LegendPosition::Bottom) { LegendPosition::Top } else { LegendPosition::Bottom };
    moved.legend = Some(Legend { source_position: Some(side), side: if side == LegendPosition::Top { Side::Top } else { Side::Bottom }, layout: None, ..legend });
    set(&mut s, moved.clone());
    let mut hidden = moved.clone();
    hidden.legend = None;
    set(&mut s, hidden.clone());
    // Shown again: the file's legend, on the right.
    let mut shown = hidden.clone();
    shown.legend = Some(Legend { source_position: Some(LegendPosition::Right), side: Side::Right, layout: None, ..chart.legend.clone().unwrap() });
    set(&mut s, shown);
    // A chart without one gets a new legend (as the file reads it); other
    // changes to a legend are refused.
    let mut s = Scenario::open("chart-legend-new", "editing/pptx-plan.pptx");
    let slide = s.p().slides[1].id;
    let id = s.document.ids().new_shape_id();
    let current = s.p().clone();
    let new_chart = s.opened.source.new_chart(&current, slide, id, EmuRect::new(0, 0, 6_000_000, 4_000_000)).unwrap();
    let index = s.p().slides[1].shapes.len();
    s.run(vec![Command::InsertShape { slide, index, shape: new_chart.clone() }]);
    let ShapeKind::Chart(c) = &new_chart.kind else { panic!("a chart") };
    let mut bare = (**c).clone();
    bare.legend = None;
    s.run(vec![Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Chart(Box::new(bare.clone()))) }]);
    let mut overlaid = (**c).clone();
    overlaid.legend.as_mut().unwrap().overlay = Some(true);
    let mut p = s.p().clone();
    p.slides[1].shapes.iter_mut().find(|x| x.id == id).unwrap().kind = ShapeKind::Chart(Box::new(overlaid));
    assert!(s.opened.source.save(&p, SaveOptions::default()).is_err());
}

/// データラベル: a chart's series' labels taken off, and values shown again
/// (as the file reads the new labels), read back the same.
#[test]
fn chart_labels_hidden_and_shown() {
    let mut s = Scenario::open("chart-labels", "benchmark/earnings-presentation.pptx");
    let (slide, shape, chart) = s
        .p()
        .slides
        .iter()
        .find_map(|sl| sl.shapes.iter().find_map(|sh| match &sh.kind {
            ShapeKind::Chart(c) if c.groups.iter().flat_map(|g| &g.series).any(|x| x.labels.is_some()) => Some((sl.id, sh.id, (**c).clone())),
            _ => None,
        }))
        .expect("a chart with labels");
    let mut off = chart.clone();
    for x in off.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
        x.labels = None;
    }
    s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(Box::new(off.clone()))) }]);
    let mut on = off.clone();
    for x in on.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
        x.labels = Some(ibis_doc_model::DataLabels { show_value: true, ..Default::default() });
    }
    let mut p = s.p().clone();
    p.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(on));
    let ShapeKind::Chart(read) = s.opened.source.as_read(&p, slide, shape).unwrap().kind else { panic!("a chart") };
    assert!(read.groups.iter().flat_map(|g| &g.series).all(|x| x.labels.as_ref().is_some_and(|l| l.show_value)));
    s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(read)) }]);
}

/// データラベルの位置（InsideEnd, Center等）や表示フラグ（show_category, show_percent）の
/// 設定・変更・保存・再読込ラウンドトリップ検証。
#[test]
fn chart_labels_positions_and_content_round_trip() {
    use ibis_doc_model::LabelPosition;
    let mut s = Scenario::open("chart-labels-pos", "benchmark/earnings-presentation.pptx");
    let (slide, shape, chart) = s
        .p()
        .slides
        .iter()
        .find_map(|sl| sl.shapes.iter().find_map(|sh| match &sh.kind {
            ShapeKind::Chart(c) if c.groups.iter().all(|g| matches!(g.kind, ibis_doc_model::ChartType::Bar { .. })) && c.groups.iter().flat_map(|g| &g.series).any(|x| x.labels.is_some()) => Some((sl.id, sh.id, (**c).clone())),
            _ => None,
        }))
        .expect("a bar chart with labels");

    // 1. ラベル位置を InsideEnd にし、category と percent を有効化して保存・再読込
    let mut modified = chart.clone();
    for x in modified.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
        x.labels = Some(ibis_doc_model::DataLabels {
            show_value: true,
            show_category: true,
            show_percent: true,
            show_series_name: false,
            position: Some(LabelPosition::InsideEnd),
            ..Default::default()
        });
    }
    let mut p = s.p().clone();
    p.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(modified));
    let ShapeKind::Chart(read) = s.opened.source.as_read(&p, slide, shape).unwrap().kind else { panic!("a chart") };
    for ser in read.groups.iter().flat_map(|g| &g.series) {
        let labels = ser.labels.as_ref().expect("labels present");
        assert!(labels.show_value);
        assert!(labels.show_category);
        assert!(labels.show_percent);
        assert_eq!(labels.position, Some(LabelPosition::InsideEnd));
    }
    s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(read.clone())) }]);

    // 2. 位置を Center に更新してインプレース更新・保存・再読込
    let mut modified2 = read.clone();
    for x in modified2.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
        if let Some(l) = &mut x.labels {
            l.position = Some(LabelPosition::Center);
        }
    }
    let mut p2 = s.p().clone();
    p2.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(modified2);
    let ShapeKind::Chart(read2) = s.opened.source.as_read(&p2, slide, shape).unwrap().kind else { panic!("a chart") };
    for ser in read2.groups.iter().flat_map(|g| &g.series) {
        let labels = ser.labels.as_ref().expect("labels present");
        assert!(labels.show_value);
        assert_eq!(labels.position, Some(LabelPosition::Center));
    }
    s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(read2)) }]);
}

/// グラフの軸の範囲と目盛（最小値・最大値・主目盛・補助目盛・表示単位・対数目盛・反転）の
/// 設定・変更・保存・再読込ラウンドトリップ検証。
#[test]
fn test_chart_axis_scaling_and_units_round_trip() {
    let mut s = Scenario::open("chart-axis-scaling", "benchmark/earnings-presentation.pptx");
    let (slide, shape, chart) = s
        .p()
        .slides
        .iter()
        .find_map(|sl| sl.shapes.iter().find_map(|sh| match &sh.kind {
            ShapeKind::Chart(c) if c.axes.iter().any(|a| a.kind == ibis_doc_model::AxisKind::Value) => Some((sl.id, sh.id, (**c).clone())),
            _ => None,
        }))
        .expect("a chart with value axis");

    // 1. 値軸の最小値(0.0)、最大値(500.0)、主目盛(50.0)、補助目盛(10.0)、表示単位(1000.0 = thousands)、反転(true) を設定
    let mut modified = chart.clone();
    for ax in &mut modified.axes {
        if ax.kind == ibis_doc_model::AxisKind::Value {
            ax.min = Some(ibis_doc_model::Number(0.0));
            ax.max = Some(ibis_doc_model::Number(500.0));
            ax.major_unit = Some(ibis_doc_model::Number(50.0));
            ax.minor_unit = Some(ibis_doc_model::Number(10.0));
            ax.display_unit = Some(ibis_doc_model::Number(1000.0));
            ax.reversed = true;
        }
    }
    let mut p = s.p().clone();
    p.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(modified));
    let ShapeKind::Chart(read) = s.opened.source.as_read(&p, slide, shape).unwrap().kind else { panic!("a chart") };
    let val_ax = read.axes.iter().find(|a| a.kind == ibis_doc_model::AxisKind::Value).expect("val axis");
    assert_eq!(val_ax.min, Some(ibis_doc_model::Number(0.0)));
    assert_eq!(val_ax.max, Some(ibis_doc_model::Number(500.0)));
    assert_eq!(val_ax.major_unit, Some(ibis_doc_model::Number(50.0)));
    assert_eq!(val_ax.minor_unit, Some(ibis_doc_model::Number(10.0)));
    assert_eq!(val_ax.display_unit, Some(ibis_doc_model::Number(1000.0)));
    assert!(val_ax.reversed);

    let saved1 = s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(read.clone())) }]);
    let reread1 = ibis_pptx::read(&saved1).unwrap();
    let ShapeKind::Chart(saved_chart1) = &reread1.slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("a chart") };
    let saved_val_ax1 = saved_chart1.axes.iter().find(|a| a.kind == ibis_doc_model::AxisKind::Value).expect("val axis");
    assert_eq!(saved_val_ax1.min, Some(ibis_doc_model::Number(0.0)));
    assert_eq!(saved_val_ax1.max, Some(ibis_doc_model::Number(500.0)));
    assert_eq!(saved_val_ax1.major_unit, Some(ibis_doc_model::Number(50.0)));
    assert_eq!(saved_val_ax1.minor_unit, Some(ibis_doc_model::Number(10.0)));
    assert_eq!(saved_val_ax1.display_unit, Some(ibis_doc_model::Number(1000.0)));
    assert!(saved_val_ax1.reversed);

    // 2. 対数目盛(logBase 10.0) を設定し、反転を false に戻す
    let mut modified2 = (*read).clone();
    for ax in &mut modified2.axes {
        if ax.kind == ibis_doc_model::AxisKind::Value {
            ax.log_base = Some(ibis_doc_model::Number(10.0));
            ax.reversed = false;
        }
    }
    let mut p2 = s.p().clone();
    p2.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(modified2));
    let ShapeKind::Chart(read2) = s.opened.source.as_read(&p2, slide, shape).unwrap().kind else { panic!("a chart") };
    let val_ax2 = read2.axes.iter().find(|a| a.kind == ibis_doc_model::AxisKind::Value).expect("val axis");
    assert_eq!(val_ax2.log_base, Some(ibis_doc_model::Number(10.0)));
    assert!(!val_ax2.reversed);

    let saved2 = s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(read2.clone())) }]);
    let reread2 = ibis_pptx::read(&saved2).unwrap();
    let ShapeKind::Chart(saved_chart2) = &reread2.slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("a chart") };
    let saved_val_ax2 = saved_chart2.axes.iter().find(|a| a.kind == ibis_doc_model::AxisKind::Value).expect("val axis");
    assert_eq!(saved_val_ax2.log_base, Some(ibis_doc_model::Number(10.0)));
    assert!(!saved_val_ax2.reversed);
}

/// グラフの系列への近似曲線（線形・指数・移動平均、数式・R-2乗値表示）および
/// 誤差範囲（両方向・正・負、標準誤差・固定値・パーセント）の設定・変更・保存・再読込ラウンドトリップ検証。
#[test]
fn test_chart_trendline_and_error_bars_round_trip() {
    use ibis_doc_model::{ErrorBarType, ErrorBarValueType, ErrorBars, Trendline, TrendlineType};

    let mut s = Scenario::open("chart-trendline-errbars", "benchmark/earnings-presentation.pptx");
    let (slide, shape, chart) = s
        .p()
        .slides
        .iter()
        .find_map(|sl| sl.shapes.iter().find_map(|sh| match &sh.kind {
            ShapeKind::Chart(c) if c.groups.iter().any(|g| !g.series.is_empty()) => Some((sl.id, sh.id, (**c).clone())),
            _ => None,
        }))
        .expect("a chart with series");

    // 1. 系列に線形近似曲線（数式表示・R-2乗値表示）と誤差範囲（両方向、パーセンテージ）を設定
    let mut modified = chart.clone();
    for ser in modified.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
        ser.trendline = Some(Trendline {
            kind: TrendlineType::Linear,
            order: 2,
            period: 2,
            disp_eq: true,
            disp_r_sqr: true,
            stroke: None,
        });
        ser.error_bars = Some(ErrorBars {
            direction: ibis_doc_model::ErrorBarDirection::Y,
            bar_type: ErrorBarType::Both,
            val_type: ErrorBarValueType::Percentage,
            no_end_cap: false,
            value: Some(ibis_doc_model::Number(5.0)),
            stroke: None,
            ..Default::default()
        });
    }

    let mut p = s.p().clone();
    p.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(modified));
    let ShapeKind::Chart(read) = s.opened.source.as_read(&p, slide, shape).unwrap().kind else { panic!("a chart") };
    for ser in read.groups.iter().flat_map(|g| &g.series) {
        let tl = ser.trendline.as_ref().expect("trendline present");
        assert_eq!(tl.kind, TrendlineType::Linear);
        assert!(tl.disp_eq);
        assert!(tl.disp_r_sqr);

        let eb = ser.error_bars.as_ref().expect("error_bars present");
        assert_eq!(eb.bar_type, ErrorBarType::Both);
        assert_eq!(eb.val_type, ErrorBarValueType::Percentage);
        assert!(!eb.no_end_cap);
    }

    let saved1 = s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(read.clone())) }]);
    let reread1 = ibis_pptx::read(&saved1).unwrap();
    let ShapeKind::Chart(saved_chart1) = &reread1.slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("a chart") };
    for ser in saved_chart1.groups.iter().flat_map(|g| &g.series) {
        let tl = ser.trendline.as_ref().expect("saved trendline present");
        assert_eq!(tl.kind, TrendlineType::Linear);
        assert!(tl.disp_eq);
        assert!(tl.disp_r_sqr);

        let eb = ser.error_bars.as_ref().expect("saved error_bars present");
        assert_eq!(eb.bar_type, ErrorBarType::Both);
        assert_eq!(eb.val_type, ErrorBarValueType::Percentage);
        assert!(!eb.no_end_cap);
    }

    // 2. 移動平均近似曲線（period 3）に変更、誤差範囲を標準誤差に変更
    let mut modified2 = (*read).clone();
    for ser in modified2.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
        if let Some(tl) = ser.trendline.as_mut() {
            tl.kind = TrendlineType::MovingAverage;
            tl.period = 3;
            tl.disp_eq = false;
            tl.disp_r_sqr = false;
        }
        if let Some(eb) = ser.error_bars.as_mut() {
            eb.bar_type = ErrorBarType::Plus;
            eb.val_type = ErrorBarValueType::StandardError;
            eb.no_end_cap = true;
        }
    }

    let mut p2 = s.p().clone();
    p2.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(modified2));
    let ShapeKind::Chart(read2) = s.opened.source.as_read(&p2, slide, shape).unwrap().kind else { panic!("a chart") };
    for ser in read2.groups.iter().flat_map(|g| &g.series) {
        let tl = ser.trendline.as_ref().expect("trendline present");
        assert_eq!(tl.kind, TrendlineType::MovingAverage);
        assert_eq!(tl.period, 3);
        assert!(!tl.disp_eq);

        let eb = ser.error_bars.as_ref().expect("error_bars present");
        assert_eq!(eb.bar_type, ErrorBarType::Plus);
        assert_eq!(eb.val_type, ErrorBarValueType::StandardError);
        assert!(eb.no_end_cap);
    }

    let saved2 = s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(read2.clone())) }]);
    let reread2 = ibis_pptx::read(&saved2).unwrap();
    let ShapeKind::Chart(saved_chart2) = &reread2.slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("a chart") };
    for ser in saved_chart2.groups.iter().flat_map(|g| &g.series) {
        let tl = ser.trendline.as_ref().expect("saved trendline present");
        assert_eq!(tl.kind, TrendlineType::MovingAverage);
        assert_eq!(tl.period, 3);

        let eb = ser.error_bars.as_ref().expect("saved error_bars present");
        assert_eq!(eb.bar_type, ErrorBarType::Plus);
        assert_eq!(eb.val_type, ErrorBarValueType::StandardError);
        assert!(eb.no_end_cap);
    }

    // 3. 近似曲線と誤差範囲を削除（None）
    let mut modified3 = (*read2).clone();
    for ser in modified3.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
        ser.trendline = None;
        ser.error_bars = None;
    }

    let mut p3 = s.p().clone();
    p3.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(modified3));
    let ShapeKind::Chart(read3) = s.opened.source.as_read(&p3, slide, shape).unwrap().kind else { panic!("a chart") };
    for ser in read3.groups.iter().flat_map(|g| &g.series) {
        assert!(ser.trendline.is_none());
        assert!(ser.error_bars.is_none());
    }

    let saved3 = s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(read3)) }]);
    let reread3 = ibis_pptx::read(&saved3).unwrap();
    let ShapeKind::Chart(saved_chart3) = &reread3.slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("a chart") };
    for ser in saved_chart3.groups.iter().flat_map(|g| &g.series) {
        assert!(ser.trendline.is_none());
        assert!(ser.error_bars.is_none());
    }
}

/// 系列の書式（塗りつぶし、枠線、マーカー）およびプロット領域の書式（背景塗りつぶし）の
/// 設定・変更・保存・再読込ラウンドトリップ検証。
#[test]
fn test_chart_series_and_plot_area_formatting_round_trip() {
    use ibis_doc_model::chart::{Marker, MarkerSymbol};

    let mut s = Scenario::open("chart-series-plot-area-fmt", "benchmark/earnings-presentation.pptx");
    let (slide, shape, chart) = s
        .p()
        .slides
        .iter()
        .find_map(|sl| sl.shapes.iter().find_map(|sh| match &sh.kind {
            ShapeKind::Chart(c) if c.groups.len() == 1 && matches!(c.groups[0].kind, ibis_doc_model::ChartType::Bar { .. }) && !c.groups[0].series.is_empty() => Some((sl.id, sh.id, (**c).clone())),
            _ => None,
        }))
        .expect("a single-group bar chart with series");

    // 1. 系列の書式（塗りつぶし、枠線、マーカー）とプロット領域の背景塗りつぶしを設定
    let mut modified = chart.clone();
    modified.groups[0].kind = ibis_doc_model::ChartType::Line;
    modified.plot_background = Fill::Solid(Color::opaque(Rgb(0xEEFFEE)));
    for ser in modified.groups.iter_mut().filter(|g|g.kind.supports_markers()).flat_map(|g| g.series.iter_mut()) {
        ser.fill = Fill::Solid(Color::opaque(Rgb(0x112233)));
        ser.stroke = Some(Stroke::solid(Color::opaque(Rgb(0x445566)), 28_575));
        ser.marker = Some(Marker {
            source: None,
            symbol: MarkerSymbol::Diamond,
            size: 9,
            fill: Fill::Solid(Color::opaque(Rgb(0x778899))),
        });
    }

    let mut p = s.p().clone();
    p.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(modified));
    let ShapeKind::Chart(read) = s.opened.source.as_read(&p, slide, shape).unwrap().kind else { panic!("a chart") };
    assert_eq!(read.plot_background, Fill::Solid(Color::opaque(Rgb(0xEEFFEE))));
    for g in &read.groups {
        for ser in &g.series {
            if matches!(g.kind, ibis_doc_model::ChartType::Line) {
                assert_eq!(ser.fill, Fill::None);
            } else {
                assert_eq!(ser.fill, Fill::Solid(Color::opaque(Rgb(0x112233))));
            }
            let st = ser.stroke.as_ref().expect("stroke present");
            assert_eq!(st.color, Color::opaque(Rgb(0x445566)));
            assert_eq!(st.width, 28_575);
            let m = ser.marker.as_ref().expect("marker present");
            assert_eq!(m.symbol, MarkerSymbol::Diamond);
            assert_eq!(m.size, 9);
            assert_eq!(m.fill, Fill::Solid(Color::opaque(Rgb(0x778899))));
        }
    }

    let saved1 = s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(read.clone())) }]);
    let reread1 = ibis_pptx::read(&saved1).unwrap();
    let ShapeKind::Chart(saved_chart1) = &reread1.slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("a chart") };
    assert_eq!(saved_chart1.plot_background, Fill::Solid(Color::opaque(Rgb(0xEEFFEE))));
    for g in &saved_chart1.groups {
        for ser in &g.series {
            if matches!(g.kind, ibis_doc_model::ChartType::Line) {
                assert_eq!(ser.fill, Fill::None);
            } else {
                assert_eq!(ser.fill, Fill::Solid(Color::opaque(Rgb(0x112233))));
            }
            let st = ser.stroke.as_ref().expect("saved stroke present");
            assert_eq!(st.color, Color::opaque(Rgb(0x445566)));
            assert_eq!(st.width, 28_575);
            let m = ser.marker.as_ref().expect("saved marker present");
            assert_eq!(m.symbol, MarkerSymbol::Diamond);
            assert_eq!(m.size, 9);
            assert_eq!(m.fill, Fill::Solid(Color::opaque(Rgb(0x778899))));
        }
    }

    // 2. 塗りつぶしなし、マーカー記号変更、プロット領域背景塗りつぶしなしに変更
    let mut modified2 = (*read).clone();
    modified2.plot_background = Fill::None;
    for ser in modified2.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
        ser.fill = Fill::None;
        if let Some(m) = ser.marker.as_mut() {
            m.symbol = MarkerSymbol::Square;
        }
    }

    let mut p2 = s.p().clone();
    p2.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(modified2));
    let ShapeKind::Chart(read2) = s.opened.source.as_read(&p2, slide, shape).unwrap().kind else { panic!("a chart") };
    assert_eq!(read2.plot_background, Fill::None);
    for ser in read2.groups.iter().flat_map(|g| &g.series) {
        assert_eq!(ser.fill, Fill::None);
        let m = ser.marker.as_ref().expect("marker present");
        assert_eq!(m.symbol, MarkerSymbol::Square);
    }

    let saved2 = s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(read2)) }]);
    let reread2 = ibis_pptx::read(&saved2).unwrap();
    let ShapeKind::Chart(saved_chart2) = &reread2.slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("a chart") };
    assert_eq!(saved_chart2.plot_background, Fill::None);
    for ser in saved_chart2.groups.iter().flat_map(|g| &g.series) {
        assert_eq!(ser.fill, Fill::None);
        let m = ser.marker.as_ref().expect("saved marker present");
        assert_eq!(m.symbol, MarkerSymbol::Square);
    }
}

/// 挿入 > グラフ: a new chart (Source::new_chart: PowerPoint's clustered
/// columns of its sample data), put on a slide and read back the same;
/// then a value changed in it.
#[test]
fn new_chart_inserted_and_edited() {
    let mut s = Scenario::open("chart-new", "editing/pptx-plan.pptx");
    let slide = s.p().slides[1].id;
    let id = s.document.ids().new_shape_id();
    let current = s.p().clone();
    let shape = s.opened.source.new_chart(&current, slide, id, EmuRect::new(1_524_000, 1_397_000, 9_144_000, 5_143_500)).unwrap();
    let ShapeKind::Chart(chart) = &shape.kind else { panic!("a chart") };
    let series: Vec<(String, usize)> = chart.groups.iter().flat_map(|g| &g.series).map(|x| (x.name.clone(), x.values.len())).collect();
    assert_eq!(series, vec![("系列 1".to_string(), 4), ("系列 2".to_string(), 4), ("系列 3".to_string(), 4)]);
    let index = s.p().slides[1].shapes.len();
    s.run(vec![Command::InsertShape { slide, index, shape: shape.clone() }]);
    let mut edited = (**chart).clone();
    edited.groups[0].series[1].values[2] = Some(ibis_doc_model::Number(9.5));
    s.run(vec![Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Chart(Box::new(edited))) }]);
}

/// 挿入 > グラフ: new charts of various kinds (Pie, Doughnut, ClusteredBar,
/// Line, Area), inserted with their templates, saved, verified in XML,
/// reloaded and checked for round-trip fidelity and Undo/Redo.
#[test]
fn new_chart_kinds_inserted_and_round_trip() {
    use ibis_pptx::NewChartKind;
    use ibis_doc_model::ChartType;

    let mut s = Scenario::open("chart-new-kinds", "editing/pptx-plan.pptx");
    let slide = s.p().slides[1].id;

    // 1. Insert Pie chart
    let pie_id = s.document.ids().new_shape_id();
    let current = s.p().clone();
    let pie_shape = s.opened.source.new_chart_kind(&current, slide, pie_id, EmuRect::new(1_000_000, 1_000_000, 5_000_000, 4_000_000), NewChartKind::Pie).unwrap();
    let ShapeKind::Chart(pie_chart) = &pie_shape.kind else { panic!("a chart") };
    assert!(matches!(pie_chart.groups[0].kind, ChartType::Pie { first_angle: 0 }));
    assert_eq!(pie_chart.groups[0].series.len(), 1);
    assert_eq!(pie_chart.groups[0].series[0].name, "売上");
    assert_eq!(pie_chart.groups[0].series[0].values.len(), 4);
    assert!(pie_chart.axes.is_empty()); // Pie has no axes!

    let index = s.p().slides[1].shapes.len();
    let saved = s.run(vec![Command::InsertShape { slide, index, shape: pie_shape.clone() }]);
    let pkg = ibis_ooxml::Package::open(saved).unwrap();
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide2.xml").unwrap().to_vec()).unwrap();
    assert!(slide_xml.contains("c:chart"));
    // Find chart part
    let chart_part = pkg.names().find(|p| p.starts_with("ppt/charts/chart") && p.ends_with(".xml")).unwrap().to_string();
    let chart_xml = String::from_utf8(pkg.read(&chart_part).unwrap().to_vec()).unwrap();
    assert!(chart_xml.contains("<c:pieChart>"));
    assert!(!chart_xml.contains("<c:valAx>"));

    // Modify pie slice angle
    let mut pie_modified = (**pie_chart).clone();
    pie_modified.groups[0].kind = ChartType::Pie { first_angle: 90 };
    let saved2 = s.run(vec![Command::SetKind { slide, shape: pie_id, kind: Box::new(ShapeKind::Chart(Box::new(pie_modified))) }]);
    let pkg2 = ibis_ooxml::Package::open(saved2).unwrap();
    let chart_xml2 = String::from_utf8(pkg2.read(&chart_part).unwrap().to_vec()).unwrap();
    assert!(chart_xml2.contains("<c:firstSliceAng val=\"90\"/>"));

    // 2. Insert Doughnut chart
    let doughnut_id = s.document.ids().new_shape_id();
    let current = s.p().clone();
    let doughnut_shape = s.opened.source.new_chart_kind(&current, slide, doughnut_id, EmuRect::new(6_000_000, 1_000_000, 5_000_000, 4_000_000), NewChartKind::Doughnut).unwrap();
    let ShapeKind::Chart(doughnut_chart) = &doughnut_shape.kind else { panic!("a chart") };
    assert!(matches!(doughnut_chart.groups[0].kind, ChartType::Doughnut { hole: 75, first_angle: 0 }));
    assert_eq!(doughnut_chart.groups[0].series.len(), 1);

    let index = s.p().slides[1].shapes.len();
    s.run(vec![Command::InsertShape { slide, index, shape: doughnut_shape.clone() }]);

    // 3. Insert Bar chart (ClusteredBar)
    let bar_id = s.document.ids().new_shape_id();
    let current = s.p().clone();
    let bar_shape = s.opened.source.new_chart_kind(&current, slide, bar_id, EmuRect::new(1_000_000, 5_500_000, 5_000_000, 4_000_000), NewChartKind::ClusteredBar).unwrap();
    let ShapeKind::Chart(bar_chart) = &bar_shape.kind else { panic!("a chart") };
    assert!(matches!(bar_chart.groups[0].kind, ChartType::Bar { horizontal: true, .. }));
    let index = s.p().slides[1].shapes.len();
    s.run(vec![Command::InsertShape { slide, index, shape: bar_shape.clone() }]);

    // 4. Insert Line chart
    let line_id = s.document.ids().new_shape_id();
    let current = s.p().clone();
    let line_shape = s.opened.source.new_chart_kind(&current, slide, line_id, EmuRect::new(6_000_000, 5_500_000, 5_000_000, 4_000_000), NewChartKind::Line).unwrap();
    let ShapeKind::Chart(line_chart) = &line_shape.kind else { panic!("a chart") };
    assert!(matches!(line_chart.groups[0].kind, ChartType::Line));
    let index = s.p().slides[1].shapes.len();
    s.run(vec![Command::InsertShape { slide, index, shape: line_shape.clone() }]);

    // 5. Insert Area chart
    let area_id = s.document.ids().new_shape_id();
    let current = s.p().clone();
    let area_shape = s.opened.source.new_chart_kind(&current, slide, area_id, EmuRect::new(1_000_000, 9_500_000, 5_000_000, 4_000_000), NewChartKind::Area).unwrap();
    let ShapeKind::Chart(area_chart) = &area_shape.kind else { panic!("a chart") };
    assert!(matches!(area_chart.groups[0].kind, ChartType::Area));
    let index = s.p().slides[1].shapes.len();
    s.run(vec![Command::InsertShape { slide, index, shape: area_shape.clone() }]);

    // Verify all 5 inserted charts exist on slide
    assert!(s.p().slides[1].shape(pie_id).is_some());
    assert!(s.p().slides[1].shape(doughnut_id).is_some());
    assert!(s.p().slides[1].shape(bar_id).is_some());
    assert!(s.p().slides[1].shape(line_id).is_some());
    assert!(s.p().slides[1].shape(area_id).is_some());

    // Undo all 6 operations and verify clean state
    for _ in 0..6 {
        assert!(s.document.undo());
    }
    assert!(s.p().slides[1].shape(pie_id).is_none());
    assert!(s.p().slides[1].shape(doughnut_id).is_none());
    assert!(s.p().slides[1].shape(bar_id).is_none());
    assert!(s.p().slides[1].shape(line_id).is_none());
    assert!(s.p().slides[1].shape(area_id).is_none());

    // Redo all 6 operations
    for _ in 0..6 {
        assert!(s.document.redo());
    }
    assert!(s.p().slides[1].shape(pie_id).is_some());
    assert!(s.p().slides[1].shape(doughnut_id).is_some());
    assert!(s.p().slides[1].shape(bar_id).is_some());
    assert!(s.p().slides[1].shape(line_id).is_some());
    assert!(s.p().slides[1].shape(area_id).is_some());
}

/// 背景グラフィックを表示しない on a slide, and a paragraph 均等割り付け
/// then 両端揃え; read back so.
#[test]
fn backdrop_hidden_and_text_distributed() {
    let mut s = Scenario::open("backdrop-dist", "editing/pptx-plan.pptx");
    let slide = s.p().slides[1].id;
    assert!(!s.p().slides[1].hide_backdrop);
    let saved = s.run(vec![Command::SetBackdropHidden { slide, hidden: true }]);
    let package = ibis_ooxml::Package::open(saved).unwrap();
    assert!(String::from_utf8(package.read("ppt/slides/slide2.xml").unwrap().to_vec()).unwrap().contains("showMasterSp=\"0\""));
    let mut target = s.p().slides[1].shapes.iter().find(|sh| sh.text_body().is_some_and(|b| b.paragraphs.len() > 1)).cloned().expect("a text");
    let shape = target.id;
    for align in [ibis_doc_model::TextAlign::Distributed, ibis_doc_model::TextAlign::Justify] {
        if let Some(b) = target.text_body_mut() {
            b.paragraphs[0].align = Some(align);
        }
        let saved = s.run(vec![Command::SetKind { slide, shape, kind: Box::new(target.kind.clone()) }]);
        let package = ibis_ooxml::Package::open(saved).unwrap();
        let xml = String::from_utf8(package.read("ppt/slides/slide2.xml").unwrap().to_vec()).unwrap();
        assert!(xml.contains(if align == ibis_doc_model::TextAlign::Justify { "algn=\"just\"" } else { "algn=\"dist\"" }));
    }
}

/// 日付と時刻 (自動的に更新): a U.S. English date field (datetime4) and a
/// Japanese one (datetime1) read, brought up to a day; written back as fields with
/// the day's text, read back the same.
#[test]
fn date_fields_updated() {
    use ibis_doc_model::{TextField, date_field::LocalTime};
    let mut package = ibis_ooxml::Package::open(std::fs::read(test_file("editing/pptx-plan.pptx")).unwrap()).unwrap();
    let xml = String::from_utf8(package.read("ppt/slides/slide1.xml").unwrap().to_vec()).unwrap();
    // The subtitle's paragraph gets the two fields.
    let at = xml.find("</a:p></p:txBody></p:sp></p:spTree>").expect("the last shape's paragraph end");
    let fields = r#"<a:fld id="{11111111-2222-4333-8444-555555555555}" type="datetime4"><a:rPr lang="en-US"/><a:t>January 2, 2020</a:t></a:fld><a:fld id="{11111111-2222-4333-8444-666666666666}" type="datetime1"><a:rPr lang="ja-JP"/><a:t>2020/1/2</a:t></a:fld>"#;
    let end_rpr = xml[..at].rfind("<a:endParaRPr").unwrap_or(at);
    let xml = format!("{}{fields}{}", &xml[..end_rpr], &xml[end_rpr..]);
    package.set_bytes("ppt/slides/slide1.xml", xml.into_bytes());
    let data = package.save(Default::default()).unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/pptx-edit");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("pptx-plan-dates.pptx"), &data).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut p = opened.presentation.clone();
    let runs = |p: &Presentation| p.slides[0].shapes.iter().flat_map(|sh| sh.text_body().into_iter().flat_map(|b| b.paragraphs.iter().flat_map(|x| x.runs.clone()))).filter(|r| r.style.field.is_some()).collect::<Vec<_>>();
    let before = runs(&p);
    assert_eq!(before.iter().map(|r| (r.text.as_str(), r.style.field)).collect::<Vec<_>>(), vec![("January 2, 2020", Some(TextField::DateTime { format: 4, east_asian: false })), ("2020/1/2", Some(TextField::DateTime { format: 1, east_asian: true }))]);
    // 2026-10-07 (a Wednesday) 13:05.
    let day = LocalTime { year: 2026, month: 10, day: 7, hour: 13, minute: 5, second: 0, weekday: 3 };
    assert!(p.refresh_dates(&day, &ibis_pptx::script_pieces));
    assert_eq!(runs(&p).iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), vec!["October 7, 2026", "2026/10/7"]);
    let saved = opened.source.save(&p, SaveOptions::default()).unwrap();
    let again = ibis_pptx::read(&saved).unwrap();
    assert_eq!(runs(&again), runs(&p));
    assert!(String::from_utf8(ibis_ooxml::Package::open(saved).unwrap().read("ppt/slides/slide1.xml").unwrap().to_vec()).unwrap().contains(r#"type="datetime4"><a:rPr lang="en-US"/><a:t>October 7, 2026</a:t>"#));
    assert_eq!(ibis_doc_model::date_field::slide_date_text(2, false, &day).as_deref(), Some("Wednesday, October 7, 2026"));
    assert_eq!(ibis_doc_model::date_field::slide_date_text(5, false, &day).as_deref(), Some("7-Oct-26"));
    assert_eq!(ibis_doc_model::date_field::slide_date_text(13, false, &day).as_deref(), Some("1:05:00 PM"));
    // Japanese, as PowerPoint showed them on 2026-10-07 (Wed) 11:37 to 11:39
    // (test/editing/pptx-datetime-by-powerpoint.pptx): the same forms at 11:38:57.
    let ja = LocalTime { hour: 11, minute: 38, second: 57, ..day };
    let texts: Vec<String> = (1..=14).map(|f| ibis_doc_model::date_field::slide_date_text(f, true, &ja).unwrap()).collect();
    assert_eq!(
        texts,
        vec!["2026/10/7", "2026年10月7日(水)", "令和8年10月7日", "2026年10月7日", "2026/10/07", "令和8年10月", "令和八年十月七日", "26/10/7 11時38分", "26/10/7 11時38分57秒", "11:38", "11:38:57", "11時38分", "11時38分57秒", "2026年"]
    );
}

/// PowerPoint's date fields (挿入 > 日付と時刻, 自動的に更新, ja-JP, made
/// on 2026-10-07): read as fields, the dates' forms (not the times', made
/// at other moments) the same as ours for that day.
#[test]
fn date_fields_by_powerpoint() {
    use ibis_doc_model::{TextField, date_field::LocalTime};
    let p = ibis_pptx::read(&std::fs::read(test_file("editing/pptx-datetime-by-powerpoint.pptx")).unwrap()).unwrap();
    let day = LocalTime { year: 2026, month: 10, day: 7, hour: 11, minute: 38, second: 0, weekday: 3 };
    let mut formats = Vec::new();
    for paragraph in p.slides.iter().flat_map(|sl| sl.shapes.iter()).flat_map(|sh| sh.text_body().into_iter().flat_map(|b| b.paragraphs.iter())) {
        // A field is its runs together (split where the script changes).
        let mut fields: Vec<(TextField, String)> = Vec::new();
        for run in &paragraph.runs {
            let Some(field) = run.style.field else { continue };
            match fields.last_mut() {
                Some((f, text)) if *f == field => text.push_str(&run.text),
                _ => fields.push((field, run.text.clone())),
            }
        }
        for (field, text) in fields {
            let TextField::DateTime { format, east_asian } = field else { continue };
            assert!(east_asian, "ja-JP");
            formats.push(format);
            if (1..=7).contains(&format) || format == 14 {
                assert_eq!(ibis_doc_model::date_field::slide_date_text(format, true, &day).as_deref(), Some(text.as_str()), "datetime{format}");
            }
        }
    }
    // Brought up to another day and saved: read back the same, each field
    // one element with the whole date.
    let opened = ibis_pptx::open(std::fs::read(test_file("editing/pptx-datetime-by-powerpoint.pptx")).unwrap()).unwrap();
    let mut q = opened.presentation.clone();
    let next = LocalTime { year: 2027, month: 1, day: 9, hour: 9, minute: 5, second: 3, weekday: 6 };
    assert!(q.refresh_dates(&next, &ibis_pptx::script_pieces));
    let saved = opened.source.save(&q, SaveOptions::default()).unwrap();
    let again = ibis_pptx::read(&saved).unwrap();
    let texts = |p: &Presentation| p.slides.iter().flat_map(|sl| sl.shapes.iter()).flat_map(|sh| sh.text_body().into_iter().flat_map(|b| b.paragraphs.iter().map(|x| x.text()))).collect::<Vec<_>>();
    assert_eq!(texts(&again), texts(&q));
    assert!(texts(&q).iter().any(|t| t.contains("令和九年一月九日")), "{:?}", texts(&q));
    let xml: String = (1..=4).filter_map(|i| ibis_ooxml::Package::open(saved.clone()).unwrap().read(&format!("ppt/slides/slide{i}.xml")).ok().map(|b| String::from_utf8(b.to_vec()).unwrap())).collect();
    assert!(xml.contains("<a:t>2027年1月9日(土)</a:t>"));
    formats.sort();
    formats.dedup();
    assert_eq!(formats, (1..=14).collect::<Vec<u8>>());
}

/// ハイパーリンク: a web page on two characters of the title (as the file
/// reads it: underlined, in the link colour), a shape to the third slide,
/// another to the next one; read back so, then taken away.
#[test]
fn hyperlinks_added_and_removed() {
    use ibis_doc_model::{CharacterFormat, SlideLink};
    let mut s = Scenario::open("links", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let title = s.p().slides[0].shapes[0].id;
    let url = SlideLink::Url("https://example.com/plan".into());
    let mut p = s.p().clone();
    p.slides[0].shapes[0].text_body_mut().unwrap().format_runs(0, 2, &CharacterFormat { link: Some(Some(url.clone())), ..Default::default() });
    let read = s.opened.source.as_read(&p, slide, title).unwrap();
    let run = read.text_body().unwrap().paragraphs[0].runs[0].clone();
    assert_eq!(run.style.link, Some(url.clone()));
    assert!(run.style.underline, "a link is underlined");
    let saved = s.run(vec![Command::SetKind { slide, shape: title, kind: Box::new(read.kind.clone()) }]);
    let xml = String::from_utf8(ibis_ooxml::Package::open(saved).unwrap().read("ppt/slides/slide1.xml").unwrap().to_vec()).unwrap();
    assert!(xml.contains("<a:hlinkClick r:id="), "{xml}");
    // Shapes of slide 3: to slide 4, and to the next.
    let third = s.p().slides[2].id;
    let target = s.p().slides[3].id;
    let (a, b) = (s.p().slides[2].shapes[1].id, s.p().slides[2].shapes[2].id);
    let saved = s.run(vec![Command::SetShapeLink { slide: third, shape: a, link: Some(SlideLink::Slide(target)) }, Command::SetShapeLink { slide: third, shape: b, link: Some(SlideLink::NextSlide) }]);
    let again = ibis_pptx::read(&saved).unwrap();
    let links: Vec<_> = again.slides[2].shapes.iter().filter_map(|sh| again.slides[2].shape_links.get(&sh.id).cloned()).collect();
    assert_eq!(links, vec![SlideLink::Slide(again.slides[3].id), SlideLink::NextSlide]);
    let xml = String::from_utf8(ibis_ooxml::Package::open(saved).unwrap().read("ppt/slides/slide3.xml").unwrap().to_vec()).unwrap();
    assert!(xml.contains("action=\"ppaction://hlinksldjump\"") && xml.contains("jump=nextslide"));
    s.run(vec![Command::SetShapeLink { slide: third, shape: a, link: None }]);
}

/// リンクのスクリーンヒント（ScreenTip / tooltip）: 図形・テキストのハイパーリンクへのヒント設定とラウンドトリップ
#[test]
fn link_tooltip_round_trip() {
    use ibis_doc_model::SlideLink;
    let mut s = Scenario::open("link-tooltip", "editing/pptx-plan.pptx");
    let third = s.p().slides[2].id;
    let target = s.p().slides[3].id;
    let shape_a = s.p().slides[2].shapes[1].id;

    // 1. 図形リンク + スクリーンヒントの設定
    let first = s.p().slides[0].id;
    let title_shape = s.p().slides[0].shapes[0].id;
    let mut p = s.p().clone();
    p.slides[0].shapes[0].text_body_mut().unwrap().format_runs(
        0,
        2,
        &ibis_doc_model::CharacterFormat {
            link: Some(Some(SlideLink::Url("https://example.com/info".into()))),
            link_tooltip: Some(Some("公式インフォメーション".into())),
            ..Default::default()
        },
    );
    let read = s.opened.source.as_read(&p, first, title_shape).unwrap();

    let saved = s.run(vec![
        Command::SetShapeLink { slide: third, shape: shape_a, link: Some(SlideLink::Slide(target)) },
        Command::SetShapeLinkTooltip { slide: third, shape: shape_a, tooltip: Some("4ページ目へジャンプ".into()) },
        Command::SetKind { slide: first, shape: title_shape, kind: Box::new(read.kind.clone()) },
    ]);

    let again = ibis_pptx::read(&saved).unwrap();
    let re_shape_a = again.slides[2].shapes[1].id;
    assert_eq!(again.slides[2].shape_links.get(&re_shape_a), Some(&SlideLink::Slide(again.slides[3].id)));
    assert_eq!(again.slides[2].shape_link_tooltips.get(&re_shape_a).map(String::as_str), Some("4ページ目へジャンプ"));

    let re_title_run = &again.slides[0].shapes[0].text_body().unwrap().paragraphs[0].runs[0];
    assert_eq!(re_title_run.style.link, Some(SlideLink::Url("https://example.com/info".into())));
    assert_eq!(re_title_run.style.link_tooltip.as_deref(), Some("公式インフォメーション"));

    let pkg = ibis_ooxml::Package::open(saved).unwrap();
    let xml3 = String::from_utf8(pkg.read("ppt/slides/slide3.xml").unwrap().to_vec()).unwrap();
    assert!(xml3.contains("tooltip=\"4ページ目へジャンプ\""), "slide3 xml should contain shape tooltip: {xml3}");
    let xml1 = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap().to_vec()).unwrap();
    assert!(xml1.contains("tooltip=\"公式インフォメーション\""), "slide1 xml should contain text tooltip: {xml1}");

    // 2. ヒントの更新・削除
    let mut p2 = s.p().clone();
    p2.slides[0].shapes[0].text_body_mut().unwrap().format_runs(
        0,
        2,
        &ibis_doc_model::CharacterFormat {
            link_tooltip: Some(None),
            ..Default::default()
        },
    );
    let read2 = s.opened.source.as_read(&p2, first, title_shape).unwrap();
    let saved2 = s.run(vec![
        Command::SetShapeLinkTooltip { slide: third, shape: shape_a, tooltip: None },
        Command::SetKind { slide: first, shape: title_shape, kind: Box::new(read2.kind.clone()) },
    ]);
    let again2 = ibis_pptx::read(&saved2).unwrap();
    let re_shape_a2 = again2.slides[2].shapes[1].id;
    assert_eq!(again2.slides[2].shape_links.get(&re_shape_a2), Some(&SlideLink::Slide(again2.slides[3].id)));
    assert_eq!(again2.slides[2].shape_link_tooltips.get(&re_shape_a2), None);

    let re_title_run2 = &again2.slides[0].shapes[0].text_body().unwrap().paragraphs[0].runs[0];
    assert_eq!(re_title_run2.style.link, Some(SlideLink::Url("https://example.com/info".into())));
    assert_eq!(re_title_run2.style.link_tooltip, None);

    let pkg2 = ibis_ooxml::Package::open(saved2).unwrap();
    let xml3_2 = String::from_utf8(pkg2.read("ppt/slides/slide3.xml").unwrap().to_vec()).unwrap();
    assert!(!xml3_2.contains("tooltip="), "slide3 xml should not contain tooltip after clearing: {xml3_2}");
    let xml1_2 = String::from_utf8(pkg2.read("ppt/slides/slide1.xml").unwrap().to_vec()).unwrap();
    assert!(!xml1_2.contains("tooltip="), "slide1 xml should not contain tooltip after clearing: {xml1_2}");
}

/// 選択ウィンドウ: a shape renamed, hidden and shown again; read back so.
#[test]
fn shapes_renamed_and_hidden() {
    let mut s = Scenario::open("selection-pane", "editing/pptx-plan.pptx");
    let slide = s.p().slides[2].id;
    let (a, b) = (s.p().slides[2].shapes[0].id, s.p().slides[2].shapes[1].id);
    assert!(s.p().slides[2].shape_names.contains_key(&a), "names are read");
    s.run(vec![Command::SetShapeName { slide, shape: a, name: "本店の図形".into() }]);
    let saved = s.run(vec![Command::SetShapeHidden { slide, shape: b, hidden: true }]);
    let p = ibis_pptx::read(&saved).unwrap();
    let sl = &p.slides[2];
    assert_eq!(sl.shape_names.get(&sl.shapes[0].id).map(String::as_str), Some("本店の図形"));
    assert!(sl.hidden_shapes.contains(&sl.shapes[1].id), "kept, hidden");
    assert_eq!(sl.shown_shapes().count(), sl.shapes.len() - 1);
    s.run(vec![Command::SetShapeHidden { slide, shape: b, hidden: false }]);
}

/// The workbook `data` with `cells` ((row, column, value) set: a number,
/// else text) changed, its first table (if any) grown over them as Excel
/// grows it, saved.
fn workbook_with(data: &[u8], cells: &[(u32, u32, &str)]) -> Vec<u8> {
    use ibis_doc_model::{CellEdit, CellInput, CellValue, SheetCommand};
    let opened = ibis_pptx::open(data.to_vec()).unwrap();
    let mut doc = Document::new(opened.presentation.clone());
    let slide = doc.presentation().slides[0].id;
    let edits = cells
        .iter()
        .map(|&(row, column, v)| CellEdit { row, column, input: CellInput::Value(v.parse::<f64>().map_or_else(|_| CellValue::Text(v.into()), CellValue::Number)) })
        .collect();
    doc.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits })]).unwrap();
    let ShapeKind::Sheet(sheet) = &doc.presentation().slides[0].shapes[0].kind else { panic!("a sheet") };
    if let Some(mut table) = sheet.tables.first().cloned() {
        for &(row, column, v) in cells {
            if column == table.range.last_column + 1 && row == table.range.first_row {
                table.columns.push(v.to_string());
            }
            table.range.last_row = table.range.last_row.max(row);
            table.range.last_column = table.range.last_column.max(column);
        }
        doc.execute(vec![Command::Sheet(SheetCommand::SetTable { slide, index: 0, table: Some(table), cells: Vec::new() })]).unwrap();
    }
    opened.source.save(doc.presentation(), SaveOptions::default()).unwrap()
}

/// データの編集 in the workbook editor, on a new chart: a value changed in
/// its workbook (テーブル1), the chart's cache and workbook rewritten.
#[test]
fn chart_data_edited_in_its_workbook() {
    let mut s = Scenario::open("chart-workbook", "editing/pptx-plan.pptx");
    let slide = s.p().slides[1].id;
    let id = s.document.ids().new_shape_id();
    let current = s.p().clone();
    let shape = s.opened.source.new_chart(&current, slide, id, EmuRect::new(1_524_000, 1_397_000, 9_144_000, 5_143_500)).unwrap();
    let index = s.p().slides[1].shapes.len();
    s.run(vec![Command::InsertShape { slide, index, shape: shape.clone() }]);
    let data = s.opened.source.chart_data(s.p(), slide, id).unwrap();
    assert_eq!(data.series[1], ibis_pptx::SeriesRefs { name: Some("Sheet1!$C$1".into()), categories: Some("Sheet1!$A$2:$A$5".into()), values: Some("Sheet1!$C$2:$C$5".into()), sizes: None });
    let workbook = workbook_with(&data.workbook, &[(3, 2, "9.5"), (0, 1, "売上")]);
    let book = ibis_pptx::read(&workbook).unwrap();
    let refs = ibis_pptx::grown_refs(&book, &data.series);
    assert_eq!(refs, data.series, "the table is as it was");
    let ShapeKind::Chart(chart) = &shape.kind else { panic!("a chart") };
    let edited = ibis_pptx::chart_from_data(chart, &book, &refs);
    assert_eq!(edited.groups[0].series[1].values[2], Some(ibis_doc_model::Number(9.5)));
    assert_eq!(edited.groups[0].series[0].name, "売上");
    s.opened.source.set_chart_data(id, edited.clone(), ibis_pptx::ChartData { workbook, series: refs });
    let saved = s.run(vec![Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Chart(Box::new(edited))) }]);
    // The workbook in the file is the one edited.
    let package = ibis_ooxml::Package::open(saved).unwrap();
    let book = package.names().find(|n| n.starts_with("ppt/embeddings/")).unwrap().to_string();
    let p = ibis_pptx::read(&package.read(&book).unwrap()).unwrap();
    let ShapeKind::Sheet(sheet) = &p.slides[0].shapes[0].kind else { panic!("a sheet") };
    assert_eq!(sheet.cell(3, 2).map(|c| c.text.as_str()), Some("9.5"));
}

/// データの編集 of a chart with no workbook: one made from its data, a
/// category and a series added in it, the chart's ranges grown to them
/// (series and points more), the workbook put in the file.
#[test]
fn chart_data_grown_in_a_new_workbook() {
    let mut s = Scenario::open("chart-grown", "benchmark/earnings-presentation.pptx");
    let (slide, shape, chart) = s
        .p()
        .slides
        .iter()
        .find_map(|sl| sl.shapes.iter().find_map(|sh| match &sh.kind {
            ShapeKind::Chart(c) if c.groups.len() == 1 && matches!(c.groups[0].kind, ibis_doc_model::ChartType::Bar { .. }) && c.groups[0].series.iter().all(|x| x.values.len() > 2 && !x.categories.is_empty()) => Some((sl.id, sh.id, (**c).clone())),
            _ => None,
        }))
        .expect("a bar chart");
    let data = s.opened.source.chart_data(s.p(), slide, shape).unwrap();
    // The workbook made has its data as a table.
    let made = ibis_pptx::read(&data.workbook).unwrap();
    let ShapeKind::Sheet(sheet) = &made.slides[0].shapes[0].kind else { panic!("a sheet") };
    assert_eq!(sheet.tables.len(), 1);
    let (series, points) = (chart.groups[0].series.len() as u32, chart.groups[0].series[0].values.len() as u32);
    assert_eq!(data.series.len(), series as usize);
    // A category (a row) and a series (a column) more.
    let mut cells = vec![(points + 1, 0, "追加の項目"), (0, series + 1, "追加の系列")];
    let numbers: Vec<String> = (0..=points + 1).map(|i| format!("{}", i + 1)).collect();
    for r in 1..=points + 1 {
        cells.push((r, series + 1, numbers[r as usize].as_str()));
        if r == points + 1 {
            for c in 1..=series {
                cells.push((r, c, "7"));
            }
        }
    }
    let workbook = workbook_with(&data.workbook, &cells);
    let book = ibis_pptx::read(&workbook).unwrap();
    let refs = ibis_pptx::grown_refs(&book, &data.series);
    assert_eq!(refs.len(), series as usize + 1);
    let target = ibis_pptx::chart_from_data(&chart, &book, &refs);
    let new_data = ibis_pptx::ChartData { workbook, series: refs };
    s.opened.source.set_chart_data(shape, target.clone(), new_data.clone());
    let mut p = s.p().clone();
    p.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(target));
    let read = s.opened.source.as_read(&p, slide, shape).unwrap();
    let ShapeKind::Chart(read_chart) = &read.kind else { panic!("a chart") };
    assert_eq!(read_chart.groups[0].series.len(), series as usize + 1);
    assert_eq!(read_chart.groups[0].series[series as usize].name, "追加の系列");
    assert_eq!(read_chart.groups[0].series[0].values.len(), points as usize + 1);
    assert_eq!(read_chart.groups[0].series[0].categories.last().map(String::as_str), Some("追加の項目"));
    s.opened.source.set_chart_data(shape, (**read_chart).clone(), new_data);
    let saved = s.run(vec![Command::SetKind { slide, shape, kind: Box::new(read.kind.clone()) }]);
    let package = ibis_ooxml::Package::open(saved).unwrap();
    assert!(package.names().any(|n| n.starts_with("ppt/embeddings/") && n.ends_with(".xlsx")));
}

/// スライドの貼り付け from another file: the benchmark's chart slide and a
/// picture slide pasted into the plan (its first layout: the plan's
/// theme), read back the same; then the pasted copy edited.
#[test]
fn slides_pasted_from_another_file() {
    let other_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/benchmark/earnings-presentation.pptx");
    let other_data: std::sync::Arc<[u8]> = std::fs::read(other_path).unwrap().into();
    let other = ibis_pptx::open(other_data.clone()).unwrap();
    let parts = other.source.slide_parts(&other.presentation);
    let mut s = Scenario::open("slide-pasted", "editing/pptx-plan.pptx");
    let layout = s.p().layouts[0].id;
    for (at, from) in [(1usize, 3usize), (2, 0)] {
        let id = s.document.ids().new_slide_id();
        let current = s.p().clone();
        let foreign = ibis_pptx::ForeignSlide { data: other_data.clone(), part: parts[from].clone().unwrap(), keep_source: false };
        let (slide, media, _) = {
            let Scenario { opened, document, .. } = &mut s;
            opened.source.paste_slide(&current, at, id, layout, foreign, || document.ids().new_shape_id()).unwrap()
        };
        // Its pictures join the model's media.
        for m in media {
            if s.p().media(m.id).is_none() {
                s.document.ids().media.push(m);
            }
        }
        assert_eq!(slide.shapes.len(), other.presentation.slides[from].shapes.len(), "its shapes");
        s.run(vec![Command::InsertSlide { index: at, slide }]);
    }
    // The pasted chart slide edits on its own.
    let pasted = s.p().slides[1].clone();
    let shape = pasted.shapes.iter().find(|sh| sh.text_body().is_some_and(|b| !b.text().is_empty())).unwrap().id;
    s.run(vec![Command::EditText { slide: pasted.id, shape, at: 0, remove: 0, insert: "貼付：".into() }]);
    // 元の書式を保持: its master (layouts, theme) comes along; the model
    // gets its layouts.
    let mut s = Scenario::open("slide-pasted-keep", "editing/pptx-plan.pptx");
    let layouts_before = s.p().layouts.len();
    let id = s.document.ids().new_slide_id();
    let current = s.p().clone();
    let foreign = ibis_pptx::ForeignSlide { data: other_data.clone(), part: parts[3].clone().unwrap(), keep_source: true };
    let (slide, media, layouts) = {
        let Scenario { opened, document, .. } = &mut s;
        opened.source.paste_slide(&current, 1, id, s_layout(&current), foreign, || document.ids().new_shape_id()).unwrap()
    };
    assert!(!layouts.is_empty() && layouts.len() <= other.presentation.layouts.len(), "the other file's master's layouts: {}", layouts.len());
    assert!(layouts.iter().any(|l| Some(l.id) == slide.layout), "the slide follows its own layout");
    for m in media {
        if s.p().media(m.id).is_none() {
            s.document.ids().media.push(m);
        }
    }
    s.document.ids().layouts.extend(layouts);
    let keep_bytes = s.run(vec![Command::InsertSlide { index: 1, slide }]);
    assert!(s.p().layouts.len() > layouts_before);
    let pkg = ibis_ooxml::Package::open(keep_bytes).unwrap();
    assert!(!pkg.names().any(|p| p.starts_with("customXml/")), "customXml parts are not leaked");
    for part in pkg.names().filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml")) {
        let xml = String::from_utf8(pkg.read(part).unwrap()).unwrap();
        assert!(!xml.contains("custDataLst"), "{part} does not contain custDataLst");
    }
}

fn s_layout(p: &Presentation) -> ibis_doc_model::LayoutId {
    p.layouts[0].id
}

/// アニメーション: effects added to a slide without any (fade in on a
/// click; the subtitle flying in with it; then the title fading out after
/// it), changed, and all taken away: the timing rebuilt, read back the same.
#[test]
fn animations_added_changed_and_removed() {
    use ibis_doc_model::{Animation, AnimationClass as C, AnimationLook as L, AnimationSide, AnimationStart as S};
    let mut s = Scenario::open("animations", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].clone();
    let (title, subtitle) = (slide.shapes[0].id, slide.shapes[1].id);
    let effect = |shape, class, start, look, duration, preset| Animation { shape, class, start, look, delay: 0, duration, preset, repeat_count: ibis_doc_model::AnimationRepeat::None, auto_reverse: false, rewind: false, accel: 0, decel: 0, after_effect: ibis_doc_model::AnimationAfterEffect::Hold, trigger: ibis_doc_model::AnimationTrigger::None , paragraph: None, chars: None, text_unit: None, path: Vec::new(), chart_element: None, chart_build: None, dgm_element: None, dgm_build: None };
    let mut a = vec![
        effect(title, C::Entrance, S::OnClick, L::Fade, 500, (10, 0)),
        effect(subtitle, C::Entrance, S::WithPrevious, L::Fly(AnimationSide::Bottom), 500, (2, 4)),
        effect(title, C::Exit, S::AfterPrevious, L::Fade, 500, (10, 0)),
    ];
    s.run(vec![Command::SetAnimations { slide: slide.id, animations: a.clone() }]);
    // Changed: the first starts with the slide, the last appears on a click.
    a[0].start = S::AfterPrevious;
    a[2] = effect(title, C::Exit, S::OnClick, L::Appear, 1, (1, 0));
    a.push(effect(subtitle, C::Emphasis, S::OnClick, L::Spin(21_600_000), 2000, (8, 0)));
    s.run(vec![Command::SetAnimations { slide: slide.id, animations: a.clone() }]);
    s.run(vec![Command::SetAnimations { slide: slide.id, animations: Vec::new() }]);
    // Kept (target/pptx-edit/pptx-plan-animations-kept.pptx, to look at).
    let mut s = Scenario::open("animations-kept", "editing/pptx-plan.pptx");
    s.run(vec![Command::SetAnimations { slide: slide.id, animations: a }]);
}

/// 図形の変更: a shape's preset changed (to an ellipse, a rounded
/// rectangle, a triangle: the file saved last has the triangle, unlike the
/// original), its text, fill and outline kept; read back the same.
#[test]
fn shape_changed_to_other_presets() {
    let mut s = Scenario::open("change-shape", "editing/pptx-plan.pptx");
    let (slide, shape) = s
        .p()
        .slides
        .iter()
        .find_map(|sl| sl.shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Auto(a) if a.text.as_ref().is_some_and(|t| !t.is_empty()) && !sl.placeholders.contains_key(&sh.id))).map(|sh| (sl.id, sh.clone())))
        .expect("an auto shape with text");
    for preset in ["ellipse", "roundRect", "triangle"] {
        let mut changed = shape.clone();
        let ShapeKind::Auto(a) = &mut changed.kind else { unreachable!() };
        a.geometry = ibis_pptx::preset_geometry(preset).unwrap();
        s.run(vec![Command::SetKind { slide, shape: shape.id, kind: Box::new(changed.kind) }]);
    }
}

/// ヘッダーとフッター: a slide number and a footer put on a slide as its
/// layout's placeholders (their place and look the layout's), read back
/// the same; taken away again.
#[test]
fn footer_and_slide_number_placeholders() {
    use ibis_doc_model::{Run, RunStyle, SlidePlaceholder, TextBody, TextField};
    let mut s = Scenario::open("footer", "editing/pptx-plan.pptx");
    let slide = s.p().slides[1].clone();
    let layout = s.p().layout(slide.layout.unwrap()).unwrap().clone();
    let mut changed = slide.clone();
    let mut ids = Vec::new();
    for (kind, text, field) in [("sldNum", "‹#›", true), ("ftr", "架空商店", false)] {
        let Some(lp) = layout.placeholders.iter().find(|p| p.placeholder.kind.as_deref() == Some(kind)) else { continue };
        let frame = layout.placement_for(&lp.placeholder).map(|p| p.0).unwrap_or_default();
        let id = s.document.ids().new_shape_id();
        let mut body = TextBody::plain("", Default::default());
        body.paragraphs[0].runs = vec![Run { text: text.into(), style: RunStyle { field: field.then_some(TextField::SlideNumber), ..Default::default() } }];
        let shape = Shape::new(id, frame, ShapeKind::Auto(AutoShape { text: Some(body), ..AutoShape::new(ibis_doc_model::Geometry::Rect, Fill::None, None) }));
        changed.shapes.push(shape);
        changed.placeholders.insert(id, SlidePlaceholder { placeholder: lp.placeholder.clone(), own_frame: false });
        ids.push(id);
    }
    assert_eq!(ids.len(), 2, "the layout has both");
    // As the file reads them.
    let mut p = s.p().clone();
    *p.slides.iter_mut().find(|x| x.id == slide.id).unwrap() = changed.clone();
    for (i, id) in ids.iter().enumerate() {
        let read = s.opened.source.as_read(&p, slide.id, *id).unwrap();
        let at = changed.shapes.iter().position(|x| x.id == *id).unwrap();
        changed.shapes[at] = read;
        assert!(changed.shapes[at].text_body().is_some(), "{i}");
    }
    s.run(vec![Command::SetSlide { slide: changed.clone() }]);
    s.run(vec![Command::RemoveShapes { slide: slide.id, shapes: ids }]);
    // Kept (target/pptx-edit/pptx-plan-footer-kept.pptx, to look at), with
    // the marks PowerPoint's すべてに適用 writes (number and footer, not on
    // title slides: test/editing/pptx-footer-by-powerpoint.pptx).
    let mut s = Scenario::open("footer-kept", "editing/pptx-plan.pptx");
    assert_eq!(s.p().header_footer, HeaderFooterMarks::default());
    let marks = HeaderFooterMarks { on_title_slides: false, header: false, date: false, footer: true, number: true };
    let saved = s.run(vec![Command::SetSlide { slide: changed }, Command::SetHeaderFooterMarks { marks }]);
    assert_eq!(ibis_pptx::read(&saved).unwrap().header_footer, marks);
    let package = ibis_ooxml::Package::open(saved).unwrap();
    let text = |name: &str| String::from_utf8(package.read(name).unwrap().to_vec()).unwrap();
    assert!(text("ppt/presentation.xml").contains(" showSpecialPlsOnTitleSld=\"0\""));
    assert!(text("ppt/slideMasters/slideMaster1.xml").contains("</p:sldLayoutIdLst><p:hf hdr=\"0\" dt=\"0\"/><p:txStyles>"));
}

/// PowerPoint's marks (挿入 > ヘッダーとフッター, すべてに適用) are read.
#[test]
fn header_footer_marks_by_powerpoint() {
    let p = ibis_pptx::read(&std::fs::read(test_file("editing/pptx-footer-by-powerpoint.pptx")).unwrap()).unwrap();
    assert_eq!(p.header_footer, HeaderFooterMarks { on_title_slides: false, header: false, date: false, footer: true, number: true });
}

#[test]
fn slides_and_shapes_together() {
    let mut s = Scenario::open("mixed", "benchmark/earnings-presentation.pptx");
    let ids: Vec<SlideId> = s.p().slides.iter().map(|s| s.id).collect();
    let layout = s.p().layouts[0].clone();
    let new_id = s.document.ids().new_slide_id();
    let shapes = new_shapes(s.document.ids());
    let mut commands = vec![
        Command::RemoveSlide { slide: ids[2] },
        Command::MoveSlide { slide: ids[5], index: 0 },
        Command::InsertSlide { index: 3, slide: Slide::from_layout(new_id, &layout) },
    ];
    for (i, shape) in shapes.into_iter().enumerate() {
        commands.push(Command::InsertShape { slide: new_id, index: i, shape });
    }
    s.run(commands);
}

/// The shapes with text of at least `min` characters, on every slide.
fn text_shapes(p: &Presentation, min: usize) -> Vec<(SlideId, ShapeId, String)> {
    p.slides
        .iter()
        .flat_map(|sl| {
            sl.shapes.iter().filter_map(move |sh| {
                let text = sh.text_body()?.text();
                (text.chars().count() >= min && matches!(sh.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_))).then(|| (sl.id, sh.id, text))
            })
        })
        .collect()
}

/// Whether `command` alone can be saved (a shape drawn over a chart cannot
/// be edited on its own).
fn saves_alone(s: &Scenario, command: &Command) -> bool {
    let mut d = s.document.clone();
    // A shape drawn over a chart is not edited on its own (the command refuses).
    if let Err(e) = d.execute(vec![command.clone()]) {
        assert!(matches!(e, ibis_doc_model::EditError::Attached(_)), "{e:?}");
        return false;
    }
    match s.opened.source.save(d.presentation(), SaveOptions::default()) {
        Ok(_) => true,
        Err(e) if e.to_string().contains("グラフの上の図形") => false,
        Err(e) => panic!("{}: {e}", s.name),
    }
}

#[test]
fn paragraphs_split_and_join() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        // Enter in the middle of every text, then Backspace joining the first two paragraphs.
        let mut s = Scenario::open("enter", file);
        let targets = text_shapes(s.p(), 4);
        assert!(!targets.is_empty());
        let commands = targets.iter().map(|(slide, shape, text)| Command::EditText { slide: *slide, shape: *shape, at: text.chars().count() / 2, remove: 0, insert: "\nあ".into() });
        let commands = commands.filter(|c| saves_alone(&s, c)).collect();
        s.run(commands);
        // (Only in bodies whose paragraphs have one indent: a line after a
        // line break is a paragraph of its own in the model, and joined to
        // another paragraph it takes that one's indent in the file, as in
        // PowerPoint, while the model keeps its own until read again.)
        let mut s = Scenario::open("join", file);
        let one_indent = |slide: SlideId, shape: ShapeId, p: &Presentation| {
            let body = p.slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap();
            body.paragraphs.windows(2).all(|w| w[0].margin_left == w[1].margin_left)
        };
        let commands: Vec<Command> = text_shapes(s.p(), 2)
            .into_iter()
            .filter(|(slide, shape, _)| one_indent(*slide, *shape, s.p()))
            .filter_map(|(slide, shape, text)| text.find('\n').map(|b| Command::EditText { slide, shape, at: text[..b].chars().count(), remove: 1, insert: String::new() }))
            .filter(|c| saves_alone(&s, c))
            .collect();
        if !commands.is_empty() {
            s.run(commands);
        }
    }
}

#[test]
fn text_style_of_whole_shapes() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("bold", file);
        let commands: Vec<Command> = text_shapes(s.p(), 1)
            .into_iter()
            .map(|(slide, shape, _)| {
                let body = s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap();
                let style = TextStyle { bold: !body.style.bold, font_size: body.style.font_size + 200, color: Rgb(0x2e75b6), align: ibis_doc_model::TextAlign::Center, ..body.style.clone() };
                Command::SetTextStyle { slide, shape, style }
            })
            .filter(|c| saves_alone(&s, c))
            .collect();
        s.run(commands);
    }
}

/// A chart with shapes drawn over it (`c:userShapes`): moving, resizing,
/// removing and restacking it takes them along.
#[test]
fn charts_with_shapes_over_them() {
    let file = "benchmark/earnings-presentation.pptx";
    let find = |p: &Presentation| {
        p.slides
            .iter()
            .find_map(|sl| sl.shapes.iter().find(|sh| !sl.attached_to(sh.id).is_empty()).map(|sh| (sl.id, sh.id, sh.frame, sl.attached_to(sh.id))))
            .expect("a chart with shapes over it")
    };
    let mut s = Scenario::open("chart-resize", file);
    let (slide, chart, frame, attached) = find(s.p());
    s.run(vec![Command::SetFrame { slide, shape: chart, frame: EmuRect::new(frame.x + 254_000, frame.y + 127_000, frame.width / 2, frame.height * 3 / 4) }]);
    // An attached shape is not edited alone.
    assert_eq!(
        s.document.execute(vec![Command::SetFrame { slide, shape: attached[0], frame: EmuRect::new(0, 0, 1, 1) }]),
        Err(ibis_doc_model::EditError::Attached(attached[0]))
    );
    // Clicking on it selects the chart.
    let sl = s.p().slide(slide).unwrap();
    let f = sl.shape(attached[0]).unwrap().frame;
    let hit = sl.hit_test(ibis_doc_model::EmuPoint::new(f.x + f.width / 2, f.y + f.height / 2));
    assert_ne!(hit, Some(attached[0]));

    let mut s = Scenario::open("chart-remove", file);
    s.run(vec![Command::RemoveShape { slide, shape: chart }]);
    assert!(attached.iter().all(|a| s.p().slide(slide).unwrap().shape(*a).is_none()));

    let mut s = Scenario::open("chart-z-order", file);
    let last = s.p().slide(slide).unwrap().shapes.len() - 1 - attached.len();
    s.run(vec![Command::MoveShape { slide, shape: chart, index: last }]);
}

/// A copy of a slide keeps the ids of the original's shapes; editing one
/// leaves the other as it is, in the model and in the saved file.
#[test]
fn a_duplicated_slide_edits_on_its_own() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("duplicate-edit", file);
        let original = s.p().slides.iter().find(|sl| sl.shapes.iter().any(|sh| sh.text_body().is_some_and(|b| b.text().chars().count() > 2))).unwrap().clone();
        let index = s.p().slide_index(original.id).unwrap();
        let copy_id = s.document.ids().new_slide_id();
        let copy = s.p().duplicate_slide(original.id, copy_id).unwrap();
        s.document.execute(vec![Command::InsertSlide { index: index + 1, slide: copy }]).unwrap();
        let shape = original.shapes.iter().find(|sh| sh.text_body().is_some_and(|b| b.text().chars().count() > 2)).unwrap().clone();
        s.run(vec![
            Command::EditText { slide: copy_id, shape: shape.id, at: 0, remove: 1, insert: "複製".into() },
            Command::SetFrame { slide: copy_id, shape: shape.id, frame: shape.frame.translated(254_000, 254_000) },
        ]);
        // The original is untouched.
        assert_eq!(s.p().slide(original.id).unwrap(), &original);
        let edited = s.p().slide(copy_id).unwrap().shape(shape.id).unwrap().clone();
        assert!(edited.text_body().unwrap().text().starts_with("複製"));
        // And so in the file.
        let saved = s.opened.source.save(s.p(), SaveOptions::default()).unwrap();
        let reread = ibis_pptx::read(&saved).unwrap();
        let (a, b) = (&reread.slides[index], &reread.slides[index + 1]);
        let pos = original.shape_index(shape.id).unwrap();
        assert_eq!(a.shapes[pos].text_body().unwrap().text(), shape.text_body().unwrap().text());
        assert_eq!(a.shapes[pos].frame, shape.frame);
        assert!(b.shapes[pos].text_body().unwrap().text().starts_with("複製"));
        assert_eq!(b.shapes[pos].frame, edited.frame);
    }
}

#[test]
fn character_format_of_a_range() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("range-format", file);
        let format = ibis_doc_model::CharacterFormat { bold: Some(true), underline: Some(true), color: Some(Rgb(0xc00000)), font_size: Some(2800), ..Default::default() };
        let commands: Vec<Command> = text_shapes(s.p(), 4)
            .into_iter()
            .map(|(slide, shape, _)| Command::SetCharacterFormat { slide, shape, start: 1, end: 3, format: format.clone() })
            .filter(|c| saves_alone(&s, c))
            .collect();
        s.run(commands);
    }
}

#[test]
#[ignore = "space_before_exact of a paragraph the edit did not touch reads back false after the save (the flag came with origin/main's reader); to fix after 0.8.5"]
fn paragraph_format_of_a_range() {
    use ibis_doc_model::{LineSpacing, ListKind, ParagraphFormat, TextAlign};
    for (name, format) in [
        ("para-align", ParagraphFormat { align: Some(TextAlign::Center), ..Default::default() }),
        ("para-spacing", ParagraphFormat { line_spacing: Some(Some(LineSpacing::Percent(150_000))), space_before: Some(1200), space_after: Some(600), ..Default::default() }),
        ("para-bullets", ParagraphFormat { list: Some(ListKind::Bullet), ..Default::default() }),
        ("para-numbers", ParagraphFormat { list: Some(ListKind::Numbered), ..Default::default() }),
        ("para-no-list", ParagraphFormat { list: Some(ListKind::None), ..Default::default() }),
        ("para-indent", ParagraphFormat { indent_by: Some(457_200), ..Default::default() }),
    ] {
        for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
            let mut s = Scenario::open(name, file);
            let commands: Vec<Command> = text_shapes(s.p(), 2)
                .into_iter()
                .map(|(slide, shape, text)| Command::SetParagraphFormat { slide, shape, start: 0, end: text.chars().count(), format: format.clone() })
                .filter(|c| saves_alone(&s, c))
                .collect();
            s.run(commands);
        }
    }
}

#[test]
fn slide_backgrounds() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("background", file);
        let ids: Vec<SlideId> = s.p().slides.iter().map(|x| x.id).collect();
        let gradient = Fill::Linear {
            angle: 5_400_000,
            scaled: false,
            stops: vec![
                ibis_doc_model::GradientStop { position: 0, color: Color::opaque(Rgb(0xdeebf7)) },
                ibis_doc_model::GradientStop { position: 100_000, color: Color::opaque(Rgb(0x9dc3e6)) },
            ],
        };
        s.run(vec![
            Command::SetBackground { slide: ids[0], fill: Fill::Solid(Color::opaque(Rgb(0xfff2cc))) },
            Command::SetBackground { slide: ids[1], fill: gradient },
        ]);
    }
}

#[test]
fn pattern_fills() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("pattern", file);
        let (slide, shape) = s
            .p()
            .slides
            .iter()
            .find_map(|sl| sl.shapes.iter().find(|sh| matches!(sh.kind, ShapeKind::Auto(_)) && !sl.is_attached(sh.id)).map(|sh| (sl.id, sh.id)))
            .unwrap();
        let fill = Fill::Pattern { preset: "dkDnDiag".into(), foreground: Color::opaque(Rgb(0x1f4e79)), background: Color::opaque(Rgb(0xdeebf7)) };
        s.run(vec![Command::SetFill { slide, shape, fill }]);
        let slide = s.p().slides[0].id;
        s.run(vec![Command::SetBackground { slide, fill: Fill::Pattern { preset: "smGrid".into(), foreground: Color::opaque(Rgb(0xd9d9d9)), background: Color::WHITE } }]);
    }
}

/// スライドをスキップ (`p:sld/@show`), and showing it again.
#[test]
fn skipped_slides() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("skip", file);
        let mut slide = s.p().slides[1].clone();
        slide.hidden = true;
        s.run(vec![Command::SetSlide { slide: slide.clone() }]);
        assert!(s.p().slides[1].hidden);
        // A copy of a skipped slide is skipped too.
        let id = s.document.ids().new_slide_id();
        let copy = s.p().duplicate_slide(slide.id, id).unwrap();
        s.run(vec![Command::InsertSlide { index: 2, slide: copy }]);
        slide.hidden = false;
        s.run(vec![Command::SetSlide { slide }]);
    }
}

/// スピーカー ノート: changed, added to a slide without any, cleared.
#[test]
fn speaker_notes() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("notes", file);
        let slides: Vec<_> = s.p().slides.iter().map(|sl| (sl.id, sl.notes.clone())).collect();
        let (with, without) = (slides.iter().find(|(_, n)| !n.is_empty()), slides.iter().find(|(_, n)| n.is_empty()));
        if let Some((id, notes)) = with {
            s.run(vec![Command::SetNotes { slide: *id, notes: format!("{notes}\n追加の行 & <記号>\n\n最後") }]);
        }
        if let Some((id, _)) = without {
            s.run(vec![Command::SetNotes { slide: *id, notes: "新しいノート\n二行目".into() }]);
        }
        if let Some((id, _)) = with {
            s.run(vec![Command::SetNotes { slide: *id, notes: String::new() }]);
        }
    }
}

/// 画面切り替え: plain, with a duration and direction, a PowerPoint 2010
/// effect, the timing alone, and none.
fn transition_namespace_case(case:&str,name:&'static str) {
    use ibis_doc_model::Transition;
    let mut s=Scenario::open(name,"editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let expected=Transition {duration:1234,..Transition::new("fade")};let saved=s.run(vec![Command::SetTransition {slide,transition:Some(expected.clone())}]);
    let mut package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:transition-namespace");let ignorable=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignorable.trim());let ac=xml.child(root,"AlternateContent").unwrap();let choice=xml.child(ac,"Choice").unwrap();let transition=xml.child(choice,"transition").unwrap();
    let foreign=match case {
        "root"=>{let n=xml.insert_element(root,0,"fixture:transition");xml.set_attr(n,"spd","slow");xml.append_element(n,"p:fade");n},
        "choice"=>{let n=xml.insert_element(root,0,"fixture:AlternateContent");let c=xml.append_element(n,"fixture:Choice");let t=xml.append_element(c,"p:transition");xml.append_element(t,"p:fade");n},
        _=>{let duration=xml.attributes(transition).iter().find(|a|a.name.ends_with(":dur")).unwrap().name.clone();xml.remove_attr(transition,&duration);xml.set_attr(transition,"fixture:dur","9999");xml.set_attr(transition,&duration,"1234");xml.set_attr(transition,"fixture:advClick","0");xml.set_attr(transition,"fixture:advTm","7777");let fade=xml.child(transition,"fade").unwrap();xml.set_attr(fade,"fixture:dir","r");xml.insert_element(transition,0,"fixture:future")},
    };xml.set_attr(foreign,"fixture:keep","opaque");let payload=xml.append_element(foreign,"fixture:payload");xml.append_text(payload,"同名を保持 😀");
    s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());assert_eq!(s.p().slides[0].transition,Some(expected.clone()));let slide=s.p().slides[0].id;let mut changed=expected;changed.duration=1357;changed.after=Some(2345);changed.on_click=false;let saved=s.run(vec![Command::SetTransition {slide,transition:Some(changed)}]);
    let package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();let foreign=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("opaque")).unwrap();assert_eq!(xml.namespace(foreign),Some("urn:ibis:test:transition-namespace"));assert_eq!(xml.text(xml.child(foreign,"payload").unwrap()),"同名を保持 😀");if case=="properties" {let parent=xml.parent(foreign).unwrap();assert_eq!(xml.attr(parent,"fixture:dur"),Some("9999"));assert_eq!(xml.attr(parent,"fixture:advTm"),Some("7777"));assert_eq!(xml.attr(parent,"fixture:advClick"),Some("0"));}
}
#[test]
fn transition_reader_ignores_foreign_root() {transition_namespace_case("root","transition-namespace-root");}
#[test]
fn transition_reader_ignores_foreign_choice() {transition_namespace_case("choice","transition-namespace-choice");}
#[test]
fn transition_reader_ignores_foreign_properties() {transition_namespace_case("properties","transition-namespace-properties");}

#[test]
fn transition_timing_keeps_source_payload() {
    use ibis_doc_model::Transition;
    for wrapped in [false,true] {
        let mut s=Scenario::open(if wrapped {"transition-unknown-wrapped"} else {"transition-unknown-plain"},"editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let saved=s.run(vec![Command::SetTransition {slide,transition:Some(Transition {duration:if wrapped {1200} else {500},..Transition::new("fade")})}]);
        let mut package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:transition");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignorable=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignorable.trim());
        let nodes:Vec<_>=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="transition").collect();for node in nodes {xml.set_attr(node,"fixture:keep","timing");let fade=xml.child(node,"fade").unwrap();xml.set_attr(fade,"thruBlk","1");let sound=xml.append_element(node,"p:sndAc");xml.append_element(sound,"p:endSnd");let payload=xml.append_element(node,"fixture:payload");xml.append_text(payload,"切替保持 😀");}
        if wrapped {let ac=xml.child(root,"AlternateContent").unwrap();xml.set_attr(ac,"fixture:keep","wrapper");let choice=xml.child(ac,"Choice").unwrap();xml.set_attr(choice,"fixture:keep","choice");}
        s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let mut transition=s.p().slides[0].transition.clone().unwrap();transition.duration=1234;transition.after=Some(2567);transition.on_click=false;let saved=s.run(vec![Command::SetTransition {slide,transition:Some(transition)}]);
        let package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();let nodes:Vec<_>=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="transition").collect();assert_eq!(nodes.len(),2);for node in nodes {assert_eq!(xml.attr(node,"fixture:keep"),Some("timing"));assert_eq!(xml.attr(xml.child(node,"fade").unwrap(),"thruBlk"),Some("1"));assert!(xml.path(node,&["sndAc","endSnd"]).is_some());assert_eq!(xml.text(xml.child(node,"payload").unwrap()),"切替保持 😀");}
        if wrapped {let ac=xml.child(root,"AlternateContent").unwrap();assert_eq!(xml.attr(ac,"fixture:keep"),Some("wrapper"));assert_eq!(xml.attr(xml.child(ac,"Choice").unwrap(),"fixture:keep"),Some("choice"));}
        let copy_id=s.document.ids().new_slide_id();let copy=s.p().duplicate_slide(slide,copy_id).unwrap();let mut transition=copy.transition.clone().unwrap();transition.duration=2000;let original=s.p().slide(slide).unwrap().clone();let saved=s.run(vec![Command::InsertSlide {index:1,slide:copy},Command::SetTransition {slide:copy_id,transition:Some(transition)}]);assert_eq!(s.p().slide(slide).unwrap(),&original);
        let package=ibis_ooxml::Package::open(saved).unwrap();let mut count=0;for name in package.names().filter(|n|n.starts_with("ppt/slides/slide")&&n.ends_with(".xml")) {let xml=package.xml(name).unwrap();let root=xml.root_element().unwrap();for node in xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("timing")) {count+=1;assert_eq!(xml.attr(xml.child(node,"fade").unwrap(),"thruBlk"),Some("1"));assert!(xml.path(node,&["sndAc","endSnd"]).is_some());assert_eq!(xml.text(xml.child(node,"payload").unwrap()),"切替保持 😀");}}assert_eq!(count,4);

    }
}

#[test]
fn transition_timing_numeric_boundaries() {
    use ibis_doc_model::Transition;
    let mut s=Scenario::open("transition-boundaries","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;
    for duration in [0,1,499,500,600,601,875,876,i32::MAX as u32,i32::MAX as u32+1,u32::MAX] {
        let transition=Transition {duration,after:Some(duration.min(i32::MAX as u32)),on_click:duration%2==0,..Transition::new("fade")};let saved=s.run(vec![Command::SetTransition {slide,transition:Some(transition)}]);std::fs::write(test_file(&format!("../target/pptx-edit/transition-bound-{duration}.pptx")),saved).unwrap();
    }
    for after in [i32::MAX as u32+1,u32::MAX] {
        let mut edited=s.p().clone();edited.slides[0].transition=Some(Transition {after:Some(after),..Transition::new("fade")});assert!(s.opened.source.save(&edited,SaveOptions::default()).is_err());
    }

}

#[test]
fn transitions() {
    use ibis_doc_model::Transition;
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("transition", file);
        let ids: Vec<_> = s.p().slides.iter().map(|sl| sl.id).collect();
        s.run(vec![Command::SetTransition { slide: ids[0], transition: Some(Transition { duration: 500, ..Transition::new("fade") }) }]);
        let push = Transition { direction: Some("u".into()), duration: 1200, after: Some(3000), ..Transition::new("push") };
        s.run(vec![Command::SetTransition { slide: ids[1], transition: Some(push) }]);
        s.run(vec![Command::SetTransition { slide: ids[2], transition: Some(Transition { on_click: false, after: Some(2000), ..Transition::new("vortex") }) }]);
        s.run(vec![Command::SetTransition { slide: ids[0], transition: None }]);
    }
}

/// セルの文字の編集: a table cell's text typed into, formatted, replaced.
#[test]
fn table_fill_keeps_foreign_fill_siblings() {
    let mut s=Scenario::open("table-fill-namespace","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();let mut table=ibis_doc_model::Table::new_grid(2,2,3_000_000,1_500_000);table.rows[0].cells[0].fill=Fill::Solid(Color::opaque(Rgb(0x112233)));let saved=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(id,EmuRect::new(500_000,500_000,3_000_000,1_500_000),ShapeKind::Table(Box::new(table)))}]);
    let mut package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:table-fill");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignorable=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignorable.trim());let pr=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.local_name(n)=="tcPr").unwrap();xml.set_attr(pr,"fixture:keep","cell");for name in ["solidFill","noFill","gradFill"] {let n=xml.append_element(pr,&format!("fixture:{name}"));xml.set_attr(n,"fixture:keep",name);let color=xml.append_element(n,"a:srgbClr");xml.set_attr(color,"val","ABCDEF");let payload=xml.append_element(n,"fixture:payload");xml.append_text(payload,"塗り保持 😀");}
    s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].id;
    for fill in [Fill::Solid(Color::opaque(Rgb(0x445566))),Fill::None,Fill::Solid(Color::opaque(Rgb(0x778899)))] {
        let ShapeKind::Table(table)=&s.p().slide(slide).unwrap().shape(shape).unwrap().kind else {panic!("table")};let mut table=(**table).clone();table.rows[0].cells[0].fill=fill;let saved=s.run(vec![Command::SetKind {slide,shape,kind:Box::new(ShapeKind::Table(Box::new(table)))}]);let package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();let pr=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("cell")).unwrap();for name in ["solidFill","noFill","gradFill"] {let n=xml.elements(pr).find(|&n|xml.namespace(n)==Some("urn:ibis:test:table-fill")&&xml.local_name(n)==name).expect("foreign fill sibling is retained");assert_eq!(xml.attr(n,"fixture:keep"),Some(name));assert_eq!(xml.attr(xml.child(n,"srgbClr").unwrap(),"val"),Some("ABCDEF"));assert_eq!(xml.text(xml.child(n,"payload").unwrap()),"塗り保持 😀");}
    }
}

#[test]
fn table_cell_text() {
    let file = "benchmark/earnings-presentation.pptx";
    let mut s = Scenario::open("table-cell", file);
    let (slide, shape, table) = s
        .p()
        .slides
        .iter()
        .find_map(|sl| sl.shapes.iter().find_map(|sh| match &sh.kind { ShapeKind::Table(t) => Some((sl.id, sh.id, t.clone())), _ => None }))
        .expect("a table");
    // A cell with text and one more.
    let cells: Vec<(usize, usize)> = table
        .rows
        .iter()
        .enumerate()
        .flat_map(|(r, row)| row.cells.iter().enumerate().filter(|(_, c)| !c.merged && !c.text.text().is_empty()).map(move |(c, _)| (r, c)))
        .take(2)
        .collect();
    let (r, c) = cells[0];
    let len = table.rows[r].cells[c].text.text().chars().count();
    let edit = Command::EditText { slide, shape, at: len, remove: 0, insert: "（改）".into() };
    s.run(vec![Command::InCell { row: r, column: c, command: Box::new(edit) }]);
    let bold = ibis_doc_model::CharacterFormat { bold: Some(true), ..Default::default() };
    s.run(vec![Command::InCell { row: r, column: c, command: Box::new(Command::SetCharacterFormat { slide, shape, start: 0, end: 1, format: bold }) }]);
    let (r2, c2) = cells[1];
    s.run(vec![Command::InCell { row: r2, column: c2, command: Box::new(Command::SetText { slide, shape, text: "置き換え\n二行目".into() }) }]);
    // セルの塗りつぶし.
    let mut filled = match &s.p().slide(slide).unwrap().shape(shape).unwrap().kind {
        ShapeKind::Table(t) => (**t).clone(),
        _ => unreachable!(),
    };
    filled.rows[r].cells[c].fill = Fill::Solid(Color::opaque(Rgb(0xffe699)));
    // セルの罫線.
    let red = Some(ibis_doc_model::Stroke::solid(Color::opaque(Rgb(0xff0000)), 19_050));
    filled.rows[r].cells[c].borders = ibis_doc_model::CellBorders { left: red.clone(), right: red.clone(), top: red.clone(), bottom: None, ..Default::default() };
    s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Table(Box::new(filled))) }]);
}

/// PPTX-OBJECT-023: 表（Table）セル内テキストへのハイパーリンク設定と保存・再読込保持
#[test]
fn table_cell_hyperlinks_round_trip() {
    use ibis_doc_model::SlideLink;
    let file = "benchmark/earnings-presentation.pptx";
    let mut s = Scenario::open("table-cell-link", file);
    let (slide, shape) = s
        .p()
        .slides
        .iter()
        .find_map(|sl| sl.shapes.iter().find_map(|sh| match &sh.kind { ShapeKind::Table(_) => Some((sl.id, sh.id)), _ => None }))
        .expect("a table");
    let target_slide = s.p().slides[0].id;
    let url_link = SlideLink::Url("https://example.com/test-table-cell".into());
    let slide_link = SlideLink::Slide(target_slide);

    let mut p = s.p().clone();
    let sl = p.slides.iter_mut().find(|sl| sl.id == slide).unwrap();
    let sh = sl.shapes.iter_mut().find(|sh| sh.id == shape).unwrap();
    let ShapeKind::Table(ref mut table) = sh.kind else { panic!("table") };
    table.rows[1].cells[1].text.format_runs(
        0,
        2,
        &ibis_doc_model::CharacterFormat {
            link: Some(Some(url_link.clone())),
            link_tooltip: Some(Some("Cell Tooltip".to_string())),
            underline: Some(true),
            ..Default::default()
        },
    );
    table.rows[1].cells[2].text.format_runs(
        0,
        2,
        &ibis_doc_model::CharacterFormat {
            link: Some(Some(slide_link.clone())),
            underline: Some(true),
            ..Default::default()
        },
    );
    let read = s.opened.source.as_read(&p, slide, shape).unwrap();

    let saved = s.run(vec![Command::SetKind {
        slide,
        shape,
        kind: Box::new(read.kind),
    }]);

    // Verify in-memory model
    let in_mem = s.p().slide(slide).unwrap().shape(shape).unwrap();
    let ShapeKind::Table(t) = &in_mem.kind else { panic!("table") };
    assert_eq!(t.rows[1].cells[1].text.paragraphs[0].runs[0].style.link, Some(url_link.clone()));
    assert_eq!(t.rows[1].cells[1].text.paragraphs[0].runs[0].style.link_tooltip, Some("Cell Tooltip".to_string()));
    assert!(t.rows[1].cells[1].text.paragraphs[0].runs[0].style.underline);
    assert_eq!(t.rows[1].cells[2].text.paragraphs[0].runs[0].style.link, Some(slide_link.clone()));
    assert!(t.rows[1].cells[2].text.paragraphs[0].runs[0].style.underline);

    // Verify saved OOXML XML contents
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let slide_part = s.p().slides.iter().position(|sl| sl.id == slide).map(|idx| format!("ppt/slides/slide{}.xml", idx + 1)).unwrap();
    let xml = String::from_utf8(package.read(&slide_part).unwrap().to_vec()).unwrap();
    assert!(xml.contains("hlinkClick"), "saved slide XML must contain hlinkClick for table cell text");
    assert!(xml.contains("tooltip=\"Cell Tooltip\""), "saved slide XML must contain cell link tooltip");

    // Verify reread
    let reopened = ibis_pptx::read(&saved).unwrap();
    let re_shape = reopened.slide(slide).unwrap().shape(shape).unwrap();
    let ShapeKind::Table(re_table) = &re_shape.kind else { panic!("table") };
    assert_eq!(re_table.rows[1].cells[1].text.paragraphs[0].runs[0].style.link, Some(url_link));
    assert_eq!(re_table.rows[1].cells[1].text.paragraphs[0].runs[0].style.link_tooltip, Some("Cell Tooltip".to_string()));
    assert!(re_table.rows[1].cells[1].text.paragraphs[0].runs[0].style.underline);
    assert_eq!(re_table.rows[1].cells[2].text.paragraphs[0].runs[0].style.link, Some(slide_link));
    assert!(re_table.rows[1].cells[2].text.paragraphs[0].runs[0].style.underline);

    // Undo / Redo
    assert!(s.document.undo());
    assert!(s.document.redo());
}

/// 行・列の挿入と削除: a row inserted (the table grows), a column removed,
/// a column inserted; the new cells as the table's style makes them.
#[test]
fn table_rows_and_columns() {
    let file = "benchmark/earnings-presentation.pptx";
    let mut s = Scenario::open("table-rows", file);
    let (index, slide, shape) = s
        .p()
        .slides
        .iter()
        .enumerate()
        .find_map(|(i, sl)| sl.shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Table(t) if !t.has_merges())).map(|sh| (i, sl.id, sh.id)))
        .expect("a table without merged cells");
    let change = |s: &mut Scenario, f: &dyn Fn(&ibis_doc_model::Table) -> Option<ibis_doc_model::Table>, grow: (i64, i64)| {
        let sh = s.p().slides[index].shape(shape).unwrap().clone();
        let ShapeKind::Table(t) = &sh.kind else { unreachable!() };
        let table = f(t).expect("the change");
        let mut changed = s.p().clone();
        let target = changed.slides[index].shapes.iter_mut().find(|x| x.id == shape).unwrap();
        target.kind = ShapeKind::Table(Box::new(table));
        target.frame.width += grow.0;
        target.frame.height += grow.1;
        let read = s.opened.source.as_read(&changed, slide, shape).unwrap();
        s.run(vec![Command::SetKind { slide, shape, kind: Box::new(read.kind.clone()) }, Command::SetFrame { slide, shape, frame: read.frame }]);
    };
    let height = |s: &Scenario| match &s.p().slides[index].shape(shape).unwrap().kind {
        ShapeKind::Table(t) => t.rows[1].height,
        _ => 0,
    };
    let h = height(&s);
    change(&mut s, &|t| t.with_row_inserted(2), (0, h));
    change(&mut s, &|t| t.with_column_removed(1), (0, 0));
    change(&mut s, &|t| t.with_column_inserted(0), (0, 0));
    // 列の幅・行の高さ.
    change(
        &mut s,
        &|t| {
            let mut t = t.clone();
            t.columns[0] += 100_000;
            t.columns[1] -= 100_000;
            t.rows[0].height += 50_000;
            Some(t)
        },
        (0, 50_000),
    );
}

/// セルの結合・分割: two by two cells merged (their text joined), then split.
#[test]
fn merged_cells() {
    let file = "benchmark/earnings-presentation.pptx";
    let mut s = Scenario::open("table-merge", file);
    let (index, slide, shape) = s
        .p()
        .slides
        .iter()
        .enumerate()
        .find_map(|(i, sl)| sl.shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Table(t) if !t.has_merges())).map(|sh| (i, sl.id, sh.id)))
        .expect("a table without merged cells");
    let table = |s: &Scenario| match &s.p().slides[index].shape(shape).unwrap().kind {
        ShapeKind::Table(t) => (**t).clone(),
        _ => unreachable!(),
    };
    let merged = table(&s).with_merged((1, 1), (2, 2)).unwrap();
    s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Table(Box::new(merged))) }]);
    let split = table(&s).with_split((1, 1)).unwrap();
    s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Table(Box::new(split))) }]);
}

/// 表の挿入: a new 3 × 4 table, and text typed into a cell.
#[test]
fn new_tables() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("new-table", file);
        let slide = s.p().slides[0].id;
        let count = s.p().slides[0].shapes.len();
        let id = s.document.ids().new_shape_id();
        let table = ibis_doc_model::Table::new_grid(3, 4, 6_096_000, 1_112_520);
        let shape = Shape::new(id, EmuRect::new(1_000_000, 2_000_000, 6_096_000, 1_112_520), ShapeKind::Table(Box::new(table)));
        s.run(vec![Command::InsertShape { slide, index: count, shape }]);
        let edit = Command::EditText { slide, shape: id, at: 0, remove: 0, insert: "見出し".into() };
        s.run(vec![Command::InCell { row: 0, column: 0, command: Box::new(edit) }]);
    }
}

#[test]
fn path_gradient_fills() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("path-gradient", file);
        let (slide, shape) = s
            .p()
            .slides
            .iter()
            .find_map(|sl| sl.shapes.iter().find(|sh| matches!(sh.kind, ShapeKind::Auto(_)) && !sl.is_attached(sh.id)).map(|sh| (sl.id, sh.id)))
            .unwrap();
        let stops = vec![
            ibis_doc_model::GradientStop { position: 0, color: Color::WHITE },
            ibis_doc_model::GradientStop { position: 100_000, color: Color::opaque(Rgb(0xed7d31)) },
        ];
        let fill = Fill::Path { path: ibis_doc_model::GradientPath::Circle, focus: [50_000, 50_000, 50_000, 50_000], stops: stops.clone() };
        s.run(vec![Command::SetFill { slide, shape, fill }]);
        let slide = s.p().slides[0].id;
        s.run(vec![Command::SetBackground { slide, fill: Fill::Path { path: ibis_doc_model::GradientPath::Rect, focus: [0, 0, 100_000, 100_000], stops } }]);
    }
}

/// Copies of existing shapes (as ⌘C ⌘V makes them: new ids, the same look).
#[test]
#[ignore = "space_before_exact of a paragraph the edit did not touch reads back false after the save (the flag came with origin/main's reader); to fix after 0.8.5"]
fn copies_of_shapes() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("copies", file);
        let slide = s.p().slides.iter().find(|sl| sl.shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Auto(_)))).unwrap().clone();
        let to = s.p().slides.last().unwrap().id;
        let count = s.p().slide(to).unwrap().shapes.len();
        let mut commands = Vec::new();
        for (i, shape) in slide.shapes.iter().filter(|sh| matches!(sh.kind, ShapeKind::Auto(_) | ShapeKind::Line(_) | ShapeKind::Picture(_)) && !slide.is_attached(sh.id)).enumerate() {
            let mut copy = shape.clone();
            copy.id = s.document.ids().new_shape_id();
            commands.push(Command::InsertShape { slide: to, index: count + i, shape: copy });
        }
        s.run(commands);
    }
}

/// レイアウトの適用: a slide's own layout again changes nothing; another
/// layout moves the placeholders that take their place from it.
#[test]
fn apply_layouts() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let s = Scenario::open("layout-same", file);
        for slide in &s.p().slides {
            let again = s.p().with_layout(slide.id, slide.layout.unwrap()).unwrap();
            assert!(again == *slide, "{}: slide {:?} changes with its own layout", s.name, slide.id);
        }
        let layouts: Vec<ibis_doc_model::LayoutId> = s.p().layouts.iter().map(|l| l.id).collect();
        for (i, layout) in layouts.iter().enumerate().take(6) {
            let mut s = Scenario::open(if i == 0 { "layout-0" } else if i == 1 { "layout-1" } else { "layout-n" }, file);
            let slide = s.p().slides[1.min(s.p().slides.len() - 1)].id;
            let new = s.opened.source.with_layout(s.p(), slide, *layout).unwrap();
            if new == *s.p().slide(slide).unwrap() {
                continue;
            }
            s.run(vec![Command::SetSlide { slide: new }]);
        }
    }
}

/// Character overrides and their removal survive save/reopen and undo/redo.
#[test]
fn extended_character_formats_and_clear() {
    let mut s = Scenario::open("extended-formats", "editing/pptx-plan.pptx");
    let (slide, shape, _) = text_shapes(s.p(), 1).into_iter().next().unwrap();
    for baseline in [30_000, -25_000] {
        s.run(vec![Command::SetCharacterFormat {
            slide, shape, start: 1, end: 3,
            format: ibis_doc_model::CharacterFormat {
                baseline: Some(baseline), spacing: Some(300),
                highlight: Some(Some(Rgb(0xffff00))), bold: Some(true),
                ..Default::default()
            },
        }]);
    }
    let mut body = s.p().slides.iter().find(|s| s.id == slide).unwrap().shape(shape).unwrap().text_body().unwrap().clone();
    let text = body.text();
    body.clear_run_formats(1, 3);
    assert_eq!(body.text(), text);
    s.run(vec![Command::SetTextBody { slide, shape, body }]);
}


/// Canvas-only sizes: shapes/layouts stay intact; all undo gives the original bytes.
#[test]
fn slide_size_round_trip() {
    let mut s = Scenario::open("slide-size", "editing/pptx-plan.pptx");
    let original = s.p().clone();
    for size in [ibis_doc_model::EmuSize::new(9_144_000, 6_858_000), ibis_doc_model::EmuSize::new(8_000_000, 8_000_000), ibis_doc_model::EmuSize::WIDESCREEN] {
        let saved = s.run(vec![Command::SetSlideSize { size }]);
        let original_package = ibis_ooxml::Package::open(s.data.clone()).unwrap();
        let saved_package = ibis_ooxml::Package::open(saved).unwrap();
        assert_eq!(original_package.names().collect::<Vec<_>>(), saved_package.names().collect::<Vec<_>>());
        for name in original_package.names().filter(|&name| name != "ppt/presentation.xml") {
            assert_eq!(original_package.read(name).unwrap(), saved_package.read(name).unwrap(), "canvas edit changes {name}");
        }
        assert_eq!(s.p().slide_size, size);
        assert_eq!(s.p().slides, original.slides);
        assert_eq!(s.p().layouts, original.layouts);
    }
}


#[test]
fn text_font_scale_ignores_foreign_mode() {
    let mut s=Scenario::open("font-scale-namespace","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();let mut body=TextBody::plain("namespace text",TextStyle::default());body.paragraphs[0].line_spacing=Some(ibis_doc_model::LineSpacing::Percent(150_000));let saved=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::TextBox(body))}]);
    let mut package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:foreign-fit");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignorable=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignorable.trim());let fit=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="noAutofit").last().unwrap();let props=xml.parent(fit).unwrap();let foreign=xml.append_element(props,"fixture:normAutofit");xml.set_attr(foreign,"fontScale","40000");xml.set_attr(foreign,"lnSpcReduction","25000");
    s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].id;let mut body=s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().clone();assert_eq!(body.font_scale,100_000);assert_eq!(body.paragraphs[0].line_spacing,Some(ibis_doc_model::LineSpacing::Percent(150_000)));body.font_scale=80_000;body.autofit=ibis_doc_model::TextAutofit::Shrink;body.paragraphs[0].line_spacing=Some(ibis_doc_model::LineSpacing::Percent(120_000));body.splice(1,0,"追記");let saved=s.run(vec![Command::SetTextBody {slide,shape,body}]);
    let package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();let foreign=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.namespace(n)==Some("urn:ibis:test:foreign-fit")&&xml.local_name(n)=="normAutofit").unwrap();assert_eq!(xml.attr(foreign,"fontScale"),Some("40000"));assert_eq!(xml.attr(foreign,"lnSpcReduction"),Some("25000"));
}

#[test]
fn text_font_scale_keeps_source_properties() {
    let mut s=Scenario::open("font-scale","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();
    let body=TextBody::plain("縮小率 test\n次の行",TextStyle::default());let shape=Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::TextBox(body));let saved=s.run(vec![Command::InsertShape {slide,index,shape}]);
    let mut package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:font-scale");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignorable=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignorable.trim());
    let fit=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="noAutofit").last().unwrap();xml.set_attr(fit,"fixture:keep","rate");let child=xml.append_element(xml.parent(fit).unwrap(),"fixture:payload");xml.append_text(child,"縮小保持 😀");
    s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].id;
    for scale in [80_000,50_000,100_000] {
        let mut body=s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().clone();body.font_scale=scale;body.autofit=if scale!=100_000{ibis_doc_model::TextAutofit::Shrink}else{ibis_doc_model::TextAutofit::None};let saved=s.run(vec![Command::SetTextBody {slide,shape,body}]);
        let package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();let fit=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("rate")).unwrap();if scale==100_000 {assert_eq!(xml.local_name(fit),"noAutofit");assert!(xml.attr(fit,"fontScale").is_none());} else {assert_eq!(xml.local_name(fit),"normAutofit");assert_eq!(xml.attr(fit,"fontScale"),Some(scale.to_string().as_str()));}let child=xml.child(xml.parent(fit).unwrap(),"payload").unwrap();assert_eq!(xml.text(child),"縮小保持 😀");
    }
    let mut package=ibis_ooxml::Package::open(s.data.clone()).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();let fit=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("rate")).unwrap();xml.rename(fit,"a:normAutofit");xml.set_attr(fit,"fontScale","80000");xml.set_attr(fit,"lnSpcReduction","10000");
    s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].id;let mut body=s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().clone();body.font_scale=50_000;body.splice(1,0,"変更");let saved=s.run(vec![Command::SetTextBody {slide,shape,body}]);
    let package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();let fit=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("rate")).unwrap();assert_eq!(xml.attr(fit,"lnSpcReduction"),Some("10000"));assert_eq!(xml.attr(fit,"fontScale"),Some("50000"));
    for duplicate in [true] {
        let mut package=ibis_ooxml::Package::open(s.data.clone()).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();let fit=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("rate")).unwrap();
        if duplicate {let props=xml.parent(fit).unwrap();xml.append_element(props,"a:noAutofit");} else {xml.detach(fit);}
        let data=package.save(SaveOptions::default()).unwrap();let opened=ibis_pptx::open(data).unwrap();let mut edited=opened.presentation.clone();edited.slides[0].shapes[index].text_body_mut().unwrap().font_scale=50_000;assert!(opened.source.save(&edited,SaveOptions::default()).is_err());
    }

    for scale in [0,999,100_001] {
        let mut edited=s.opened.presentation.clone();edited.slides[0].shapes[index].text_body_mut().unwrap().font_scale=scale;assert!(s.opened.source.save(&edited,SaveOptions::default()).is_err());
        let mut copy=edited.slides[0].shapes[index].clone();copy.id=s.document.ids().new_shape_id();assert!(ibis_pptx::save::check_shape_insertion(&copy).is_err());
    }

}

#[test]
fn text_autofit_round_trip() {
    use ibis_doc_model::TextAutofit;
    let mut s = Scenario::open("text-autofit", "editing/pptx-plan.pptx");
    let (slide, shape, _) = text_shapes(s.p(), 1).into_iter().next().unwrap();

    // 1. ResizeShape (<a:spAutoFit/>) に変更して保存・再読込
    let mut body = s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().clone();
    body.autofit = TextAutofit::ResizeShape;
    let saved = s.run(vec![Command::SetTextBody { slide, shape, body: body.clone() }]);
    let opened = ibis_pptx::open(saved).unwrap();
    let reloaded_body = opened.presentation.slides[0].shape(shape).unwrap().text_body().unwrap();
    assert_eq!(reloaded_body.autofit, TextAutofit::ResizeShape);

    // 2. Shrink (<a:normAutofit/>) に変更して保存・再読込
    body.autofit = TextAutofit::Shrink;
    body.font_scale = 75_000;
    let saved = s.run(vec![Command::SetTextBody { slide, shape, body: body.clone() }]);
    let opened = ibis_pptx::open(saved).unwrap();
    let reloaded_body = opened.presentation.slides[0].shape(shape).unwrap().text_body().unwrap();
    assert_eq!(reloaded_body.autofit, TextAutofit::Shrink);
    assert_eq!(reloaded_body.font_scale, 75_000);

    // 3. None (<a:noAutofit/>) に変更して保存・再読込
    body.autofit = TextAutofit::None;
    body.font_scale = 100_000;
    let saved = s.run(vec![Command::SetTextBody { slide, shape, body: body.clone() }]);
    let opened = ibis_pptx::open(saved).unwrap();
    let reloaded_body = opened.presentation.slides[0].shape(shape).unwrap().text_body().unwrap();
    assert_eq!(reloaded_body.autofit, TextAutofit::None);

    // 4. Undo / Redo の確認
    assert!(s.document.undo());
    assert_eq!(s.p().slides[0].shape(shape).unwrap().text_body().unwrap().autofit, TextAutofit::Shrink);

    assert!(s.document.undo());
    assert_eq!(s.p().slides[0].shape(shape).unwrap().text_body().unwrap().autofit, TextAutofit::ResizeShape);

    assert!(s.document.redo());
    assert_eq!(s.p().slides[0].shape(shape).unwrap().text_body().unwrap().autofit, TextAutofit::Shrink);

    assert!(s.document.redo());
    assert_eq!(s.p().slides[0].shape(shape).unwrap().text_body().unwrap().autofit, TextAutofit::None);
}

#[test]
fn text_body_properties_round_trip() {
    let mut s = Scenario::open("text-body", "editing/pptx-plan.pptx");
    let (slide, shape, _) = text_shapes(s.p(), 1).into_iter().next().unwrap();
    for (anchor, vertical, sideways) in [(ibis_doc_model::VerticalAnchor::Middle, true, 0), (ibis_doc_model::VerticalAnchor::Bottom, false, 5_400_000), (ibis_doc_model::VerticalAnchor::Top, false, 16_200_000), (ibis_doc_model::VerticalAnchor::Top, false, 0)] {
        let mut body = s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().clone();
        body.anchor = anchor; body.vertical = vertical; body.sideways = sideways;
        body.wrap = !body.wrap;
        body.insets = ibis_doc_model::Insets { left: 0, top: 0, right: 0, bottom: 0 };
        s.run(vec![Command::SetTextBody { slide, shape, body }]);
    }
    let mut body = s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().clone();
    body.anchor = ibis_doc_model::VerticalAnchor::Middle;
    body.splice(1, 0, "追記\n");
    s.run(vec![Command::SetTextBody { slide, shape, body }]);
}


#[test]
fn reset_placeholder_placement() {
    let mut s = Scenario::open("reset-placement", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let original = s.p().reset_slide_frames(slide).unwrap();
    let shape = *original.placeholders.iter().find(|(_, ph)| !ph.own_frame).unwrap().0;
    let frame = s.p().slide(slide).unwrap().shape(shape).unwrap().frame;
    s.run(vec![Command::SetFrame { slide, shape, frame: EmuRect { x: frame.x + 400_000, y: frame.y + 200_000, ..frame } }, Command::SetTransform { slide, shape, rotation: 5_400_000, flip_h: true, flip_v: true }]);
    let reset = s.p().reset_slide_frames(slide).unwrap();
    assert_eq!(reset.shape(shape), original.shape(shape));
    s.run(vec![Command::SetSlide { slide: reset }]);
    assert!(!s.p().slide(slide).unwrap().placeholders[&shape].own_frame);
}


#[test]
fn slide_number_cached_text_updates_after_move() {
    const NS: &str = "urn:ibis:test:field-cache";
    fn check(data: &[u8], value: &str) -> String {
        let package = ibis_ooxml::Package::open(data.to_vec()).unwrap(); let xml = package.xml("ppt/slides/slide1.xml").unwrap(); let root = xml.root_element().unwrap();
        let field = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.attr(n,"fixture:flag") == Some("field-cache")).unwrap();
        assert_eq!(xml.attr(field,"type"),Some("slidenum"));
        assert_eq!(xml.text(xml.child(field,"t").unwrap()),value);
        let payload = xml.elements(field).find(|&n| xml.namespace(n) == Some(NS)).unwrap(); assert_eq!(xml.text(payload),"keep 😀");
        let cs = xml.path(field,&["rPr","cs"]).unwrap(); assert_eq!(xml.attr(cs,"typeface"),Some("+mn-cs")); assert_eq!(xml.attr(cs,"fixture:unknown"),Some("unchanged"));
        xml.attr(field,"id").unwrap().to_string()
    }
    let mut s = Scenario::open("field-cache-move", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id; let index = s.p().slides[0].shapes.len(); let id = s.document.ids().new_shape_id();
    let mut body = TextBody::plain("1", TextStyle::default()); body.paragraphs[0].runs[0].style.field = Some(ibis_doc_model::TextField::SlideNumber);
    let shape = Shape::new(id, EmuRect::new(1_000_000,1_000_000,1_000_000,500_000),ShapeKind::TextBox(body));
    let saved = s.run(vec![Command::InsertShape { slide, index, shape }]);
    let mut package = ibis_ooxml::Package::open(saved).unwrap(); let xml = package.xml_mut("ppt/slides/slide1.xml").unwrap(); let root = xml.root_element().unwrap();
    xml.set_attr(root,"xmlns:fixture",NS); xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");
    let ignorable = format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or("")); xml.set_attr(root,"mc:Ignorable",ignorable.trim());
    let field = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n)=="fld" && xml.attr(n,"type")==Some("slidenum")).unwrap();
    xml.set_attr(field,"fixture:flag","field-cache"); let payload = xml.append_element(field,"fixture:payload"); xml.append_text(payload,"keep 😀");
    let pr = xml.child(field,"rPr").unwrap(); let cs = xml.child(pr,"cs").unwrap_or_else(|| xml.append_element(pr,"a:cs")); xml.set_attr(cs,"typeface","+mn-cs"); xml.set_attr(cs,"fixture:unknown","unchanged");
    s.data = package.save(SaveOptions::default()).unwrap(); let field_id = check(&s.data,"1");
    s.opened = ibis_pptx::open(s.data.clone()).unwrap(); s.document = Document::new(s.opened.presentation.clone()); let slide = s.p().slides[0].id;
    let saved = s.run(vec![Command::MoveSlide { slide, index: 1 }]); assert_eq!(check(&saved,"2"),field_id);
}

#[test]
fn picture_crop_round_trip() {
    let mut s = Scenario::open("picture-crop", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let picture = new_shapes(s.document.ids()).pop().unwrap();
    let index = s.p().slides[0].shapes.len();
    let saved = s.run(vec![Command::InsertShape { slide, index, shape: picture }]);
    // Reopen: exercise edits of an existing p:pic, not just new-picture serialization.
    s.opened = ibis_pptx::open(saved.clone()).unwrap();
    s.data = saved;
    s.document = Document::new(s.opened.presentation.clone());
    let slide = s.p().slides[0].id;
    let shape = s.p().slides[0].shapes[index].id;
    for crop in [ibis_doc_model::Crop { left: 12_000, top: 7_000, right: 23_000, bottom: 16_000 }, ibis_doc_model::Crop { left: -10_000, ..Default::default() }, ibis_doc_model::Crop::default()] {
        let mut picture = match &s.p().slide(slide).unwrap().shape(shape).unwrap().kind { ShapeKind::Picture(p) => p.clone(), _ => unreachable!() };
        picture.crop = crop;
        s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Picture(picture)) }]);
    }
}


#[test]
fn shape_shadow_edit_keeps_unrelated_effects() {
    const NS:&str="urn:ibis:test:shadow-edit";
    fn check(data:&[u8],shadow:bool) {
        let package=ibis_ooxml::Package::open(data.to_vec()).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();let list=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:flag")==Some("shadow-effects")).unwrap();
        assert_eq!(xml.elements(list).filter(|&n|xml.namespace(n)==Some(NS)&&xml.local_name(n)=="outerShdw").count(),1);assert!(xml.child(list,"glow").is_some());
        let nodes:Vec<_>=xml.elements(list).filter(|&n|xml.namespace(n)==Some("http://schemas.openxmlformats.org/drawingml/2006/main")&&xml.local_name(n)=="outerShdw").collect();assert_eq!(nodes.len(),usize::from(shadow));
        if shadow {assert_eq!(xml.attr(nodes[0],"fixture:keep"),Some("node"));assert_eq!(xml.attr(nodes[0],"fixture:unknown"),Some("120000"));let payload=xml.elements(nodes[0]).find(|&n|xml.namespace(n)==Some(NS)).unwrap();assert_eq!(xml.text(payload),"shadow opaque 😀");}
    }
    let mut s=Scenario::open("shadow-edit-unknown","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();let mut auto=AutoShape::new(Geometry::Rect,Fill::Solid(Color::BLACK),None);auto.glow=Some((Color::opaque(Rgb(0x4472C4)),63_500));auto.shadow=Some(ibis_doc_model::Shadow::simple(Color::BLACK,0,38100,0));let shape=Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto));let saved=s.run(vec![Command::InsertShape {slide,index,shape}]);
    let mut package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture",NS);xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignorable=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignorable.trim());let list=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.local_name(n)=="effectLst"&&xml.child(n,"outerShdw").is_some()).unwrap();xml.set_attr(list,"fixture:flag","shadow-effects");let shadow=xml.child(list,"outerShdw").unwrap();xml.set_attr(shadow,"fixture:keep","node");xml.set_attr(shadow,"fixture:unknown","120000");let payload=xml.append_element(shadow,"fixture:payload");xml.append_text(payload,"shadow opaque 😀");let foreign=xml.append_element(list,"fixture:outerShdw");xml.set_attr(foreign,"rad","7777");let color=xml.append_element(foreign,"a:srgbClr");xml.set_attr(color,"val","ABCDEF");s.data=package.save(SaveOptions::default()).unwrap();check(&s.data,true);s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].clone();
    for value in [Some(ibis_doc_model::Shadow::simple(Color::BLACK,50_800,76_200,2_700_000)),Some(ibis_doc_model::Shadow::simple(Color {rgb:Rgb(0x123456),alpha:128},25_400,50_800,5_400_000)),None] {
        let mut auto=match &s.p().slide(slide).unwrap().shape(shape.id).unwrap().kind {ShapeKind::Auto(a)=>a.clone(),_=>panic!("auto")};auto.shadow=value;let saved=s.run(vec![Command::SetKind {slide,shape:shape.id,kind:Box::new(ShapeKind::Auto(auto))}]);check(&saved,value.is_some());
    }
}

#[test]
fn shadow_numeric_boundaries() {
    let mut s=Scenario::open("shadow-numeric","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();let auto=AutoShape::new(Geometry::Rect,Fill::Solid(Color::BLACK),None);let saved=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto))}]);
    s.data=saved;s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].id;
    for (case,blur,distance) in [("zero",0,0),("blur32",i32::MAX as i64,0),("dist32",0,i32::MAX as i64),("both32",i32::MAX as i64,i32::MAX as i64)] {
        let ShapeKind::Auto(mut auto)=s.p().slides[0].shapes[index].kind.clone() else {panic!("auto")};auto.shadow=Some(ibis_doc_model::Shadow::simple(Color::BLACK,blur,distance,21_599_999));let saved=s.run(vec![Command::SetKind {slide,shape,kind:Box::new(ShapeKind::Auto(auto))}]);std::fs::write(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../target/pptx-edit/shadow-bound-{case}.pptx")),saved).unwrap();
    }
    for (blur,distance,direction) in [(-1,0,0),(0,-1,0),(i32::MAX as i64+1,0,0),(0,i32::MAX as i64+1,0),(0,i64::MAX,0),(0,0,-1),(0,0,21_600_000)] {
        let mut shape=s.p().slides[0].shapes[index].clone();let ShapeKind::Auto(a)=&mut shape.kind else {panic!("auto")};a.shadow=Some(ibis_doc_model::Shadow::simple(Color::BLACK,blur,distance,direction));assert!(ibis_pptx::save::check_shape_insertion(&shape).is_err());let mut preview=s.p().clone();preview.apply(Command::SetKind {slide,shape:shape.id,kind:Box::new(shape.kind)}).unwrap();assert!(s.opened.source.save(&preview,SaveOptions::default()).is_err());
    }

}

#[test]
fn glow_radius_limits_reject_invalid_saves() {
    let mut s=Scenario::open("glow-radius-limits","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();let mut auto=AutoShape::new(Geometry::Rect,Fill::Solid(Color::BLACK),None);auto.glow=Some((Color::BLACK,63_500));let saved=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto))}]);
    s.data=saved;s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].clone();
    for radius in [-1,i32::MAX as i64+1] {
        let mut changed=shape.clone();let ShapeKind::Auto(a)=&mut changed.kind else {panic!("auto")};a.glow=Some((Color::BLACK,radius));assert!(ibis_pptx::save::check_shape_insertion(&changed).is_err());let mut preview=s.p().clone();preview.apply(Command::SetKind {slide,shape:shape.id,kind:Box::new(changed.kind.clone())}).unwrap();assert!(s.opened.source.save(&preview,SaveOptions::default()).is_err());
    }
    let ShapeKind::Auto(mut a)=shape.kind else {panic!("auto")};a.glow=Some((Color::BLACK,i32::MAX as i64));s.run(vec![Command::SetKind {slide,shape:shape.id,kind:Box::new(ShapeKind::Auto(a))}]);
}

#[test]
fn shape_glow_edit_keeps_unrelated_effects() {
    const NS: &str="urn:ibis:test:glow-edit";
    fn check(data:&[u8],glow:bool) {
        let package=ibis_ooxml::Package::open(data.to_vec()).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();
        let list=xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.attr(n,"fixture:flag")==Some("effects")).unwrap();
        assert_eq!(xml.elements(list).filter(|&n| xml.namespace(n)==Some(NS) && xml.local_name(n)=="glow").count(),1);
        assert_eq!(xml.attr(xml.child(list,"softEdge").unwrap(),"rad"),Some("12345"));
        let nodes:Vec<_>=xml.elements(list).filter(|&n| xml.namespace(n)==Some("http://schemas.openxmlformats.org/drawingml/2006/main") && xml.local_name(n)=="glow").collect();assert_eq!(nodes.len(),usize::from(glow));
        if glow {assert_eq!(xml.attr(nodes[0],"fixture:keep"),Some("node"));let payload=xml.elements(nodes[0]).find(|&n| xml.namespace(n)==Some(NS)).unwrap();assert_eq!(xml.text(payload),"opaque 😀");}
    }
    let mut s=Scenario::open("glow-edit-unknown","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();
    let mut auto=AutoShape::new(Geometry::Rect,Fill::Solid(Color::BLACK),None);auto.glow=Some((Color::opaque(Rgb(0x4472C4)),63_500));auto.shadow=Some(ibis_doc_model::Shadow::simple(Color::BLACK,0,38100,0));
    let shape=Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto));let saved=s.run(vec![Command::InsertShape {slide,index,shape}]);
    let mut package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture",NS);xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignorable=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignorable.trim());
    let list=xml.descendants(root).into_iter().find(|&n| xml.is_element(n)&&xml.local_name(n)=="effectLst"&&xml.child(n,"glow").is_some()).unwrap();xml.set_attr(list,"fixture:flag","effects");let glow=xml.child(list,"glow").unwrap();xml.set_attr(glow,"fixture:keep","node");let payload=xml.append_element(glow,"fixture:payload");xml.append_text(payload,"opaque 😀");let foreign=xml.append_element(list,"fixture:glow");xml.set_attr(foreign,"rad","7777");let color=xml.append_element(foreign,"a:srgbClr");xml.set_attr(color,"val","ABCDEF");let soft=xml.append_element(list,"a:softEdge");xml.set_attr(soft,"rad","12345");
    s.data=package.save(SaveOptions::default()).unwrap();check(&s.data,true);s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].clone();
    for value in [Some((Color::opaque(Rgb(0x4472C4)),127_000)),Some((Color {rgb:Rgb(0x12ABCD),alpha:96},90_488)),Some((Color::BLACK,i32::MAX as i64)),None] {
        let mut auto=match &s.p().slide(slide).unwrap().shape(shape.id).unwrap().kind {ShapeKind::Auto(a)=>a.clone(),_=>panic!("auto")};auto.glow=value;let saved=s.run(vec![Command::SetKind {slide,shape:shape.id,kind:Box::new(ShapeKind::Auto(auto))}]);check(&saved,value.is_some());
    }
}

#[test]
fn copy_shape_shadow_and_glow_round_trip() {
    let mut s = Scenario::open("copy-shape-effects", "editing/pptx-plan.pptx"); let slide = s.p().slides[0].id;
    for effects in 0..3 {
        let mut auto = AutoShape::new(Geometry::Rect,Fill::Solid(Color::opaque(Rgb(0x4472C4))),None);
        auto.text = Some(TextBody::plain("effects",TextStyle::default()));
        if effects != 1 { auto.shadow = Some(ibis_doc_model::Shadow::simple(Color { rgb: Rgb(0x123456),alpha:128 }, 101_600,152_400,8_100_000)); }
        if effects != 0 { auto.glow = Some((Color { rgb: Rgb(0x66CCFF),alpha:200 },63_500)); }
        let id = s.document.ids().new_shape_id(); let shape = Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto));
        let index = s.p().slides[0].shapes.len(); s.run(vec![Command::InsertShape { slide,index,shape }]);
    }
}

#[test]
fn copy_shape_soft_edge_round_trip() {
    // a:softEdge on a new shape is written and read back (alone and with a glow).
    let mut s = Scenario::open("copy-shape-soft-edge", "editing/pptx-plan.pptx"); let slide = s.p().slides[0].id;
    for (soft, glow) in [(63_500, false), (12_700, true)] {
        let mut auto = AutoShape::new(Geometry::Rect,Fill::Solid(Color::opaque(Rgb(0x4472C4))),None);
        auto.soft_edge = Some(soft);
        if glow { auto.glow = Some((Color::opaque(Rgb(0xFFC000)),63_500)); }
        let id = s.document.ids().new_shape_id(); let shape = Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto));
        let index = s.p().slides[0].shapes.len(); s.run(vec![Command::InsertShape { slide,index,shape }]);
        let ShapeKind::Auto(read) = &s.p().slides[0].shapes[index].kind else { panic!("auto") };
        assert_eq!(read.soft_edge, Some(soft));
    }
}

#[test]
fn copy_shape_reflection_round_trip() {
    // a:reflection on a new shape is written and read back (PEFF's two kinds).
    let mut s = Scenario::open("copy-shape-reflection", "editing/pptx-plan.pptx"); let slide = s.p().slides[0].id;
    let tight = ibis_doc_model::Reflection { blur: 6_350, start_alpha: 52_000, end_alpha: 300, start_pos: 0, end_pos: 35_000, distance: 0, direction: 5_400_000, fade_direction: 5_400_000, sx: 100_000, sy: -100_000, kx: 0, ky: 0, align: ibis_doc_model::TileAlign::BottomLeft, rot_with_shape: false };
    let apart = ibis_doc_model::Reflection { blur: 12_700, start_alpha: 38_000, end_alpha: 0, end_pos: 65_000, distance: 50_800, ..tight };
    for reflection in [tight, apart] {
        let mut auto = AutoShape::new(Geometry::Rect,Fill::Solid(Color::opaque(Rgb(0x4472C4))),None);
        auto.reflection = Some(reflection);
        let id = s.document.ids().new_shape_id(); let shape = Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto));
        let index = s.p().slides[0].shapes.len(); s.run(vec![Command::InsertShape { slide,index,shape }]);
    }
}

#[test]
fn existing_shape_reflection_and_soft_edge_round_trip() {
    let mut s = Scenario::open("existing-shape-reflection-soft-edge", "editing/pptx-plan.pptx");
    let slide = s.p().slides[2].id;
    let shape_id = s.p().slides[2].shapes.iter().find(|sh| s.p().slides[2].shape_names.get(&sh.id).map(|s| s.as_str()) == Some("Shape_0")).unwrap().id;

    // 1. 既存図形に反射とソフトエッジを設定
    let tight = ibis_doc_model::Reflection { blur: 6_350, start_alpha: 52_000, end_alpha: 300, start_pos: 0, end_pos: 35_000, distance: 0, direction: 5_400_000, fade_direction: 5_400_000, sx: 100_000, sy: -100_000, kx: 0, ky: 0, align: ibis_doc_model::TileAlign::BottomLeft, rot_with_shape: false };
    let ShapeKind::Auto(mut auto) = s.p().slides[2].shape(shape_id).unwrap().kind.clone() else { panic!() };
    auto.reflection = Some(tight);
    auto.soft_edge = Some(63_500);
    let bytes = s.run(vec![Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Auto(auto)) }]);

    let reread = ibis_pptx::read(&bytes).unwrap();
    let ShapeKind::Auto(reopened) = &reread.slides[2].shape(shape_id).unwrap().kind else { panic!() };
    assert_eq!(reopened.reflection, Some(tight));
    assert_eq!(reopened.soft_edge, Some(63_500));

    // 2. 反射とソフトエッジを解除 (None)
    let ShapeKind::Auto(mut auto) = s.p().slides[2].shape(shape_id).unwrap().kind.clone() else { panic!() };
    auto.reflection = None;
    auto.soft_edge = None;
    let bytes_cleared = s.run(vec![Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Auto(auto)) }]);

    let reread_cleared = ibis_pptx::read(&bytes_cleared).unwrap();
    let ShapeKind::Auto(reopened_cleared) = &reread_cleared.slides[2].shape(shape_id).unwrap().kind else { panic!() };
    assert_eq!(reopened_cleared.reflection, None);
    assert_eq!(reopened_cleared.soft_edge, None);
}

#[test]
fn existing_shape_bevel_round_trip() {
    let mut s = Scenario::open("existing-shape-bevel", "editing/pptx-plan.pptx");
    let slide = s.p().slides[2].id;
    let shape_id = s.p().slides[2].shapes.iter().find(|sh| s.p().slides[2].shape_names.get(&sh.id).map(|s| s.as_str()) == Some("Shape_0")).unwrap().id;

    // 1. 既存図形に面取りを設定（Circle）
    let bevel_circle = ibis_doc_model::Bevel::standard(ibis_doc_model::BevelPreset::Circle);
    let ShapeKind::Auto(mut auto) = s.p().slides[2].shape(shape_id).unwrap().kind.clone() else { panic!() };
    auto.bevel = Some(bevel_circle);
    let bytes = s.run(vec![Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Auto(auto)) }]);

    let reread = ibis_pptx::read(&bytes).unwrap();
    let ShapeKind::Auto(reopened) = &reread.slides[2].shape(shape_id).unwrap().kind else { panic!() };
    assert_eq!(reopened.bevel, Some(bevel_circle));

    // 2. プリセットとサイズを変更（Angle, 127_000, 100_000）
    let bevel_angle = ibis_doc_model::Bevel::new(ibis_doc_model::BevelPreset::Angle, 127_000, 100_000);
    let ShapeKind::Auto(mut auto) = s.p().slides[2].shape(shape_id).unwrap().kind.clone() else { panic!() };
    auto.bevel = Some(bevel_angle);
    let bytes_changed = s.run(vec![Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Auto(auto)) }]);

    let reread_changed = ibis_pptx::read(&bytes_changed).unwrap();
    let ShapeKind::Auto(reopened_changed) = &reread_changed.slides[2].shape(shape_id).unwrap().kind else { panic!() };
    assert_eq!(reopened_changed.bevel, Some(bevel_angle));

    // 3. 他のプリセット（Slope, SoftRound, HardEdge）も検証
    for preset in [ibis_doc_model::BevelPreset::Slope, ibis_doc_model::BevelPreset::SoftRound, ibis_doc_model::BevelPreset::HardEdge] {
        let bevel = ibis_doc_model::Bevel::standard(preset);
        let ShapeKind::Auto(mut auto) = s.p().slides[2].shape(shape_id).unwrap().kind.clone() else { panic!() };
        auto.bevel = Some(bevel);
        let b = s.run(vec![Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Auto(auto)) }]);
        let r = ibis_pptx::read(&b).unwrap();
        let ShapeKind::Auto(sh) = &r.slides[2].shape(shape_id).unwrap().kind else { panic!() };
        assert_eq!(sh.bevel, Some(bevel));
    }

    // 4. 面取りを解除 (None)
    let ShapeKind::Auto(mut auto) = s.p().slides[2].shape(shape_id).unwrap().kind.clone() else { panic!() };
    auto.bevel = None;
    let bytes_cleared = s.run(vec![Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Auto(auto)) }]);

    let reread_cleared = ibis_pptx::read(&bytes_cleared).unwrap();
    let ShapeKind::Auto(reopened_cleared) = &reread_cleared.slides[2].shape(shape_id).unwrap().kind else { panic!() };
    assert_eq!(reopened_cleared.bevel, None);
}

#[test]
fn copy_picture_color_effects_round_trip() {
    let mut s = Scenario::open("copy-picture-colors", "editing/pptx-picture-color-effects.pptx");
    let source = s.p().slides[0].shapes.iter().find(|shape| matches!(&shape.kind,ShapeKind::Picture(p) if p.duotone.is_some() && p.clear.is_some())).unwrap().clone();
    let slide = s.p().slides[0].id;
    for effects in 0..3 {
        let mut copy = source.clone(); copy.id = s.document.ids().new_shape_id();
        let ShapeKind::Picture(picture) = &mut copy.kind else { panic!("picture") };
        match effects { 0 => picture.clear = None, 1 => picture.duotone = None, _ => {} }
        let index = s.p().slides[0].shapes.len();
        s.run(vec![Command::InsertShape { slide, index, shape: copy }]);
    }
}

#[test]
fn reset_picture_color_effects_keeps_unknown_blip_content() {
    const NS: &str = "urn:ibis:test:picture-color-reset";
    fn check(data: &[u8], effects: usize) {
        let package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
        let xml = package.xml("ppt/slides/slide1.xml").unwrap();
        let root = xml.root_element().unwrap();
        let blip = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "blip" && xml.attr(n, "fixture:flag") == Some("keep")).unwrap();
        assert_eq!(xml.attr(blip, "fixture:flag"), Some("keep"));
        assert_eq!(xml.elements(blip).filter(|&n| xml.namespace(n) == Some(NS)).count(), 2);
        for n in xml.elements(blip).filter(|&n| xml.namespace(n) == Some(NS)) {
            assert_eq!(xml.attr(n,"fixture:payload"),Some("opaque"));
            if xml.local_name(n)=="duotone" {
                let values:Vec<_>=xml.elements(n).map(|c|xml.attr(c,"val").unwrap()).collect();assert_eq!(values,vec!["ABCDEF","123456"]);
            } else {
                let from=xml.path(n,&["clrFrom","srgbClr"]).unwrap();assert_eq!(xml.attr(from,"val"),Some("ABCDEF"));
                let alpha=xml.path(n,&["clrTo","srgbClr","alpha"]).unwrap();assert_eq!(xml.attr(alpha,"val"),Some("0"));
            }
        }
        assert_eq!(xml.elements(blip).filter(|&n| xml.namespace(n) == Some("http://schemas.openxmlformats.org/drawingml/2006/main") && matches!(xml.local_name(n), "duotone" | "clrChange")).count(), effects);
    }
    let mut s = Scenario::open("picture-color-reset", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id; let index = s.p().slides[0].shapes.len();
    let picture = new_shapes(s.document.ids()).pop().unwrap();
    let saved = s.run(vec![Command::InsertShape { slide, index, shape: picture }]);
    let mut package = ibis_ooxml::Package::open(saved).unwrap();
    let xml = package.xml_mut("ppt/slides/slide1.xml").unwrap(); let root = xml.root_element().unwrap();
    xml.set_attr(root, "xmlns:fixture", NS); xml.set_attr(root, "xmlns:mc", "http://schemas.openxmlformats.org/markup-compatibility/2006"); xml.set_attr(root, "mc:Ignorable", "fixture");
    let blip = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "blip").unwrap();
    xml.set_attr(blip, "fixture:flag", "keep");
    let duo = xml.append_element(blip, "a:duotone");
    for color in ["112233", "DDEEFF"] { let n = xml.append_element(duo, "a:srgbClr"); xml.set_attr(n,"val",color); }
    let change = xml.append_element(blip, "a:clrChange");
    let from = xml.append_element(change,"a:clrFrom"); let color = xml.append_element(from,"a:srgbClr"); xml.set_attr(color,"val","FF0000");
    let to = xml.append_element(change,"a:clrTo"); let color = xml.append_element(to,"a:srgbClr"); xml.set_attr(color,"val","FF0000"); let alpha = xml.append_element(color,"a:alpha"); xml.set_attr(alpha,"val","0");
    for local in ["duotone", "clrChange"] {
        let node = xml.append_element(blip, &format!("fixture:{local}")); xml.set_attr(node,"fixture:payload","opaque");
        if local == "duotone" {
            for value in ["ABCDEF", "123456"] { let color=xml.append_element(node,"a:srgbClr");xml.set_attr(color,"val",value); }
        } else {
            let from=xml.append_element(node,"a:clrFrom");let color=xml.append_element(from,"a:srgbClr");xml.set_attr(color,"val","ABCDEF");
            let to=xml.append_element(node,"a:clrTo");let color=xml.append_element(to,"a:srgbClr");xml.set_attr(color,"val","ABCDEF");let alpha=xml.append_element(color,"a:alpha");xml.set_attr(alpha,"val","0");
        }
    }
    s.data = package.save(SaveOptions::default()).unwrap(); check(&s.data,2);
    std::fs::write(test_file("../target/pptx-edit/picture-color-effects-original.pptx"), &s.data).unwrap();
    s.opened = ibis_pptx::open(s.data.clone()).unwrap(); s.document = Document::new(s.opened.presentation.clone());
    let slide = s.p().slides[0].id; let shape = s.p().slides[0].shapes[index].clone();
    let ShapeKind::Picture(mut picture) = shape.kind else { panic!("picture") }; assert!(picture.duotone.is_some() && picture.clear.is_some());
    picture.duotone = None;
    let saved = s.run(vec![Command::SetKind { slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture.clone())) }]);
    check(&saved,1);
    let reread = ibis_pptx::read(&saved).unwrap(); let ShapeKind::Picture(current) = &reread.slides[0].shapes[index].kind else { panic!("picture") };
    assert!(current.duotone.is_none() && current.clear.is_some());
    picture.clear = None;
    let saved = s.run(vec![Command::SetKind { slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) }]);
    check(&saved,0);
}

#[test]
fn picture_luminance_round_trip() {
    let mut s = Scenario::open("picture-luminance", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let mut picture_shape = new_shapes(s.document.ids()).pop().unwrap();
    let ShapeKind::Picture(picture) = &mut picture_shape.kind else { panic!("picture") };
    picture.luminance = Some((20_000, 40_000));
    let index = s.p().slides[0].shapes.len();
    let saved = s.run(vec![Command::InsertShape { slide, index, shape: picture_shape.clone() }]);

    // Verify XML has <a:lum bright="20000" contrast="40000"/>
    let package = ibis_ooxml::Package::open(saved).unwrap();
    let xml = package.xml("ppt/slides/slide1.xml").unwrap();
    let root = xml.root_element().unwrap();
    let lum = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "lum").expect("lum element");
    assert_eq!(xml.attr(lum, "bright"), Some("20000"));
    assert_eq!(xml.attr(lum, "contrast"), Some("40000"));

    // Modify existing picture's luminance to (-30_000, 0)
    let ShapeKind::Picture(mut p2) = picture_shape.kind.clone() else { panic!("picture") };
    p2.luminance = Some((-30_000, 0));
    let saved = s.run(vec![Command::SetKind { slide, shape: picture_shape.id, kind: Box::new(ShapeKind::Picture(p2)) }]);
    let package = ibis_ooxml::Package::open(saved).unwrap();
    let xml = package.xml("ppt/slides/slide1.xml").unwrap();
    let root = xml.root_element().unwrap();
    let lum = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "lum").expect("lum element");
    assert_eq!(xml.attr(lum, "bright"), Some("-30000"));
    assert_eq!(xml.attr(lum, "contrast"), None);

    // Reset luminance to None
    let ShapeKind::Picture(mut p3) = picture_shape.kind else { panic!("picture") };
    p3.luminance = None;
    let saved = s.run(vec![Command::SetKind { slide, shape: picture_shape.id, kind: Box::new(ShapeKind::Picture(p3)) }]);
    let package = ibis_ooxml::Package::open(saved).unwrap();
    let xml = package.xml("ppt/slides/slide1.xml").unwrap();
    let root = xml.root_element().unwrap();
    assert!(xml.descendants(root).into_iter().all(|n| !xml.is_element(n) || xml.local_name(n) != "lum"));
}

#[test]
fn picture_clear_transparent_color_round_trip() {
    let mut s = Scenario::open("picture-clear", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let mut picture_shape = new_shapes(s.document.ids()).pop().unwrap();
    let ShapeKind::Picture(picture) = &mut picture_shape.kind else { panic!("picture") };
    picture.clear = Some(ibis_doc_model::Rgb(0xffffff));
    let index = s.p().slides[0].shapes.len();
    let saved = s.run(vec![Command::InsertShape { slide, index, shape: picture_shape.clone() }]);

    // Verify XML has <a:clrChange> with clrFrom=FFFFFF and clrTo=FFFFFF alpha=0
    let package = ibis_ooxml::Package::open(saved).unwrap();
    let xml = package.xml("ppt/slides/slide1.xml").unwrap();
    let root = xml.root_element().unwrap();
    let change = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "clrChange").expect("clrChange element");
    let from = xml.path(change, &["clrFrom", "srgbClr"]).expect("clrFrom srgbClr");
    assert_eq!(xml.attr(from, "val"), Some("FFFFFF"));
    let to_alpha = xml.path(change, &["clrTo", "srgbClr", "alpha"]).expect("clrTo alpha");
    assert_eq!(xml.attr(to_alpha, "val"), Some("0"));

    // Modify existing picture's clear color to black (0x000000)
    let ShapeKind::Picture(mut p2) = picture_shape.kind.clone() else { panic!("picture") };
    p2.clear = Some(ibis_doc_model::Rgb(0x000000));
    let saved = s.run(vec![Command::SetKind { slide, shape: picture_shape.id, kind: Box::new(ShapeKind::Picture(p2)) }]);
    let package = ibis_ooxml::Package::open(saved).unwrap();
    let xml = package.xml("ppt/slides/slide1.xml").unwrap();
    let root = xml.root_element().unwrap();
    let change = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "clrChange").expect("clrChange element");
    let from = xml.path(change, &["clrFrom", "srgbClr"]).expect("clrFrom srgbClr");
    assert_eq!(xml.attr(from, "val"), Some("000000"));

    // Reset clear color to None
    let ShapeKind::Picture(mut p3) = picture_shape.kind else { panic!("picture") };
    p3.clear = None;
    let saved = s.run(vec![Command::SetKind { slide, shape: picture_shape.id, kind: Box::new(ShapeKind::Picture(p3)) }]);
    let package = ibis_ooxml::Package::open(saved).unwrap();
    let xml = package.xml("ppt/slides/slide1.xml").unwrap();
    let root = xml.root_element().unwrap();
    assert!(xml.descendants(root).into_iter().all(|n| !xml.is_element(n) || xml.local_name(n) != "clrChange"));
}

#[test]
fn picture_artistic_effect_round_trip() {
    let mut s = Scenario::open("picture-artistic", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let mut picture_shape = new_shapes(s.document.ids()).pop().unwrap();
    let ShapeKind::Picture(picture) = &mut picture_shape.kind else { panic!("picture") };
    picture.artistic_effect = Some(ArtisticEffect::marker());
    let index = s.p().slides[0].shapes.len();
    let saved = s.run(vec![Command::InsertShape { slide, index, shape: picture_shape.clone() }]);

    // Verify XML has <a:ext uri="{BEBA8EAE-BF5A-486C-A8C5-ECC9F3942E4B}"> and <a14:artisticMarker>
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = package.xml("ppt/slides/slide1.xml").unwrap();
    let root = xml.root_element().unwrap();
    let marker = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "artisticMarker").expect("artisticMarker element");
    assert_eq!(xml.attr(marker, "size"), Some("15"));
    assert_eq!(xml.attr(marker, "trans"), Some("0"));

    // Verify reopening deserializes artistic_effect
    let reopened = ibis_pptx::open(saved).unwrap();
    let last_shape = reopened.presentation.slides[0].shapes.last().unwrap();
    let ShapeKind::Picture(p) = &last_shape.kind else { panic!("picture") };
    assert_eq!(p.artistic_effect, Some(ArtisticEffect::marker()));

    // Modify existing picture's artistic effect to ArtisticEffect::glass()
    let ShapeKind::Picture(mut p2) = picture_shape.kind.clone() else { panic!("picture") };
    p2.artistic_effect = Some(ArtisticEffect::glass());
    let saved = s.run(vec![Command::SetKind { slide, shape: picture_shape.id, kind: Box::new(ShapeKind::Picture(p2)) }]);
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = package.xml("ppt/slides/slide1.xml").unwrap();
    let root = xml.root_element().unwrap();
    let glass = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "artisticGlass").expect("artisticGlass element");
    assert_eq!(xml.attr(glass, "scaling"), Some("25"));
    assert_eq!(xml.attr(glass, "trans"), Some("0"));
    assert!(xml.descendants(root).into_iter().all(|n| !xml.is_element(n) || xml.local_name(n) != "artisticMarker"));

    let reopened = ibis_pptx::open(saved).unwrap();
    let last_shape = reopened.presentation.slides[0].shapes.last().unwrap();
    let ShapeKind::Picture(p) = &last_shape.kind else { panic!("picture") };
    assert_eq!(p.artistic_effect, Some(ArtisticEffect::glass()));

    // Reset artistic effect to None
    let ShapeKind::Picture(mut p3) = picture_shape.kind else { panic!("picture") };
    p3.artistic_effect = None;
    let saved = s.run(vec![Command::SetKind { slide, shape: picture_shape.id, kind: Box::new(ShapeKind::Picture(p3)) }]);
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = package.xml("ppt/slides/slide1.xml").unwrap();
    let root = xml.root_element().unwrap();
    assert!(xml.descendants(root).into_iter().all(|n| !xml.is_element(n) || xml.local_name(n) != "artisticGlass"));
    assert!(xml.descendants(root).into_iter().all(|n| !xml.is_element(n) || xml.attr(n, "uri") != Some("{BEBA8EAE-BF5A-486C-A8C5-ECC9F3942E4B}")));

    let reopened = ibis_pptx::open(saved).unwrap();
    let last_shape = reopened.presentation.slides[0].shapes.last().unwrap();
    let ShapeKind::Picture(p) = &last_shape.kind else { panic!("picture") };
    assert_eq!(p.artistic_effect, None);
}

#[test]
fn replace_existing_picture() {
    let mut s = Scenario::open("replace-picture", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let picture = new_shapes(s.document.ids()).pop().unwrap();
    let index = s.p().slides[0].shapes.len();
    let saved = s.run(vec![Command::InsertShape { slide, index, shape: picture }]);
    s.opened = ibis_pptx::open(saved.clone()).unwrap();
    s.data = saved;
    s.document = Document::new(s.opened.presentation.clone());
    let slide = s.p().slides[0].id;
    let original = s.p().slides[0].shapes[index].clone();
    let media = s.document.ids().add_media(MediaFormat::Png, png(7, 5, [0xff, 0x44, 0x22]));
    let ShapeKind::Picture(mut picture) = original.kind.clone() else { panic!("picture") };
    picture.media = media;
    s.run(vec![Command::SetKind { slide, shape: original.id, kind: Box::new(ShapeKind::Picture(picture)) }]);
    assert_eq!(s.p().slide(slide).unwrap().shape(original.id).unwrap().frame, original.frame);
}


#[test]
fn replace_match_keeps_first_run_format() {
    let mut s = Scenario::open("replace-match", "editing/pptx-plan.pptx");
    let (slide, shape, _) = text_shapes(s.p(), 1).into_iter().next().unwrap();
    s.run(vec![Command::SetCharacterFormat { slide, shape, start: 1, end: 2, format: ibis_doc_model::CharacterFormat { bold: Some(true), baseline: Some(30_000), spacing: Some(300), ..Default::default() } }]);
    s.run(vec![Command::ReplaceText { slide, shape, at: 1, remove: 2, insert: "替換".into() }]);
    let body = s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap();
    let inserted = body.paragraphs[0].runs.iter().find(|r| r.text.contains("替換")).unwrap();
    assert!(body.style.with_run(&inserted.style).bold);
    assert_eq!((inserted.style.baseline, inserted.style.spacing), (30_000, 300));
}

#[test]
fn existing_cell_frame_round_trip() {
    let mut s = Scenario::open("cell-frame", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let index = s.p().slides[0].shapes.len();
    let id = s.document.ids().new_shape_id();
    let table = ibis_doc_model::Table::new_grid(2, 2, 6_096_000, 1_112_520);
    let shape = Shape::new(id, EmuRect::new(1_000_000, 2_000_000, 6_096_000, 1_112_520), ShapeKind::Table(Box::new(table)));
    let saved = s.run(vec![Command::InsertShape { slide, index, shape }]);
    s.opened = ibis_pptx::open(saved.clone()).unwrap();
    s.data = saved;
    s.document = Document::new(s.opened.presentation.clone());
    let slide = s.p().slides[0].id;
    let shape = s.p().slides[0].shapes[index].id;
    let ShapeKind::Table(table) = &s.p().slides[0].shapes[index].kind else { panic!("table") };
    let mut body = table.rows[0].cells[0].text.clone();
    body.insets = ibis_doc_model::Insets { left: 36_000, top: 72_000, right: 108_000, bottom: 144_000 };
    body.anchor = ibis_doc_model::VerticalAnchor::Middle;
    s.run(vec![Command::InCell { row: 0, column: 0, command: Box::new(Command::SetTextBody { slide, shape, body }) }]);
}

#[test]
fn cell_frame_keeps_injected_namespace_content() {
    const NS: &str = "urn:ibis:test:tcpr-preservation";
    fn check(data: &[u8]) {
        let package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
        let xml = package.xml("ppt/slides/slide1.xml").unwrap();
        let root = xml.root_element().unwrap();
        let pr = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "tcPr" && xml.attr(n, "fixture:flag") == Some("opaque & unchanged")).unwrap();
        assert_eq!(xml.lookup_prefix(pr, NS).as_deref(), Some("fixture"));
        let payload = xml.children(pr).iter().copied().find(|&n| xml.is_element(n) && xml.namespace(n) == Some(NS) && xml.local_name(n) == "payload").unwrap();
        assert_eq!(xml.attr(payload, "fixture:version"), Some("17"));
        let item = xml.children(payload).iter().copied().find(|&n| xml.is_element(n) && xml.local_name(n) == "item").unwrap();
        assert_eq!(xml.namespace(item), Some(NS));
        assert_eq!(xml.attr(item, "key"), Some("unknown-key"));
        assert_eq!(xml.text(item), "未知の値 <keep> & 😀");
        assert_eq!(xml.children(pr).iter().filter(|&&n| xml.is_element(n) && xml.namespace(n) == Some(NS)).count(), 1);
    }
    let mut s = Scenario::open("cell-frame-unknown", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let index = s.p().slides[0].shapes.len();
    let id = s.document.ids().new_shape_id();
    let table = ibis_doc_model::Table::new_grid(2, 2, 6_096_000, 1_112_520);
    let shape = Shape::new(id, EmuRect::new(1_000_000, 2_000_000, 6_096_000, 1_112_520), ShapeKind::Table(Box::new(table)));
    let saved = s.run(vec![Command::InsertShape { slide, index, shape }]);
    let mut package = ibis_ooxml::Package::open(saved).unwrap();
    let xml = package.xml_mut("ppt/slides/slide1.xml").unwrap();
    let root = xml.root_element().unwrap();
    xml.set_attr(root, "xmlns:fixture", NS);
    xml.set_attr(root, "xmlns:mc", "http://schemas.openxmlformats.org/markup-compatibility/2006");
    let ignorable = format!("{} fixture", xml.attr(root, "mc:Ignorable").unwrap_or(""));
    xml.set_attr(root, "mc:Ignorable", ignorable.trim());
    let pr = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "tcPr").unwrap();
    xml.set_attr(pr, "fixture:flag", "opaque & unchanged");
    let payload = xml.append_element(pr, "fixture:payload");
    xml.set_attr(payload, "fixture:version", "17");
    let item = xml.append_element(payload, "fixture:item");
    xml.set_attr(item, "key", "unknown-key");
    xml.append_text(item, "未知の値 <keep> & 😀");
    s.data = package.save(ibis_ooxml::SaveOptions::default()).unwrap();
    check(&s.data);
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let slide = s.p().slides[0].id;
    let shape = s.p().slides[0].shapes[index].id;
    for anchor in [ibis_doc_model::VerticalAnchor::Middle, ibis_doc_model::VerticalAnchor::Bottom, ibis_doc_model::VerticalAnchor::Top] {
        let ShapeKind::Table(table) = &s.p().slides[0].shapes[index].kind else { panic!("table") };
        let mut body = table.rows[0].cells[0].text.clone();
        body.anchor = anchor;
        body.insets = ibis_doc_model::Insets { left: 36_000, top: 72_000, right: 108_000, bottom: 144_000 };
        let saved = s.run(vec![Command::InCell { row: 0, column: 0, command: Box::new(Command::SetTextBody { slide, shape, body }) }]);
        check(&saved);
        check(&s.opened.source.save(s.p(), SaveOptions { regenerate_xml: true, ..Default::default() }).unwrap());
        assert!(s.document.undo());
        check(&s.opened.source.save(s.p(), SaveOptions::default()).unwrap());
        assert!(s.document.redo());
        check(&s.opened.source.save(s.p(), SaveOptions::default()).unwrap());
    }
}

#[test]
fn character_outline_round_trip() {
    let mut s = Scenario::open("character-outline", "editing/pptx-plan.pptx");
    let (slide, shape, _) = text_shapes(s.p(), 1).into_iter().next().unwrap();
    for outline in [Some((Rgb(0x112233), 12_700)), Some((Rgb(0x995522), 25_400)), None] {
        s.run(vec![Command::SetCharacterFormat { slide, shape, start: 1, end: 3, format: ibis_doc_model::CharacterFormat { outline: Some(outline), ..Default::default() } }]);
    }
    s.run(vec![Command::SetCharacterFormat { slide, shape, start: 1, end: 3, format: ibis_doc_model::CharacterFormat { outline: Some(Some((Rgb(0x112233), 12_700))), ..Default::default() } }]);
    // Also exercise generated run serialization after a paragraph split.
    s.run(vec![Command::EditText { slide, shape, at: 2, remove: 0, insert: "\n".into() }]);
}

#[test]
fn character_shadow_round_trip() {
    use ibis_doc_model::{Color, Shadow, TileAlign};
    let mut s = Scenario::open("character-shadow", "editing/pptx-plan.pptx");
    let (slide, shape, _) = text_shapes(s.p(), 1).into_iter().next().unwrap();

    let shadow_preset = Shadow {
        color: Color { rgb: Rgb(0x334455), alpha: 102 },
        blur: 25_400,
        distance: 19_050,
        direction: 5_400_000,
        sx: 100_000,
        sy: 100_000,
        kx: 0,
        ky: 0,
        align: TileAlign::TopLeft,
        rot_with_shape: true,
    };

    // 1. 影を設定して保存・再読込
    let saved = s.run(vec![Command::SetCharacterFormat {
        slide,
        shape,
        start: 1,
        end: 3,
        format: ibis_doc_model::CharacterFormat {
            shadow: Some(Some(shadow_preset)),
            ..Default::default()
        },
    }]);

    let opened = ibis_pptx::open(saved).unwrap();
    let r = opened.presentation.slides[0].shape(shape).unwrap().text_body().unwrap().paragraphs[0].runs.iter().find(|r| r.style.shadow.is_some()).expect("should have shadow run");
    assert_eq!(r.style.shadow, Some(shadow_preset));

    // 2. 影を解除（None）して保存・再読込
    let saved = s.run(vec![Command::SetCharacterFormat {
        slide,
        shape,
        start: 1,
        end: 3,
        format: ibis_doc_model::CharacterFormat {
            shadow: Some(None),
            ..Default::default()
        },
    }]);

    let opened = ibis_pptx::open(saved).unwrap();
    let has_shadow = opened.presentation.slides[0].shape(shape).unwrap().text_body().unwrap().paragraphs[0].runs.iter().any(|r| r.style.shadow.is_some());
    assert!(!has_shadow, "shadow should be removed");

    // 3. Undo / Redo での動作確認
    assert!(s.document.undo());
    let r = s.p().slides[0].shape(shape).unwrap().text_body().unwrap().paragraphs[0].runs.iter().find(|r| r.style.shadow.is_some()).expect("undo should restore shadow");
    assert_eq!(r.style.shadow, Some(shadow_preset));

    assert!(s.document.redo());
    let has_shadow = s.p().slides[0].shape(shape).unwrap().text_body().unwrap().paragraphs[0].runs.iter().any(|r| r.style.shadow.is_some());
    assert!(!has_shadow, "redo should remove shadow again");
}

#[test]
fn existing_line_cap_compound_and_end_sizes() {
    use ibis_doc_model::{LineAlignment, LineCap, LineCompound};
    let mut s = Scenario::open("line-properties", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let index = s.p().slides[0].shapes.len();
    let line = new_shapes(s.document.ids()).into_iter().find(|s| matches!(s.kind, ShapeKind::Line(_))).unwrap();
    let saved = s.run(vec![Command::InsertShape { slide, index, shape: line }]);
    s.opened = ibis_pptx::open(saved.clone()).unwrap();
    s.data = saved;
    s.document = Document::new(s.opened.presentation.clone());
    let slide = s.p().slides[0].id;
    let shape = s.p().slides[0].shapes[index].id;
    for cap in [LineCap::Flat, LineCap::Round, LineCap::Square] {
        for compound in [LineCompound::Single, LineCompound::Double, LineCompound::ThickThin, LineCompound::ThinThick, LineCompound::Triple] {
            for alignment in [LineAlignment::Center, LineAlignment::Inset] {
                let ShapeKind::Line(mut stroke) = s.p().slides[0].shapes[index].kind.clone() else { panic!("line") };
                stroke.cap = cap; stroke.compound = compound; stroke.alignment = alignment;
                stroke.head = LineEnd::Triangle(EndSize { width: 0, length: 2 });
                stroke.tail = LineEnd::Stealth(EndSize { width: 2, length: 0 });
                s.run(vec![Command::SetStroke { slide, shape, stroke: Some(stroke) }]);
            }
        }
    }
}

#[test]
fn stroke_alignment_round_trip() {
    use ibis_doc_model::LineAlignment;
    let mut s = Scenario::open("stroke-alignment", "editing/pptx-plan.pptx");
    let (slide, shape, mut stroke) = s
        .p()
        .slides
        .iter()
        .find_map(|sl| {
            sl.shapes.iter().find_map(|sh| match &sh.kind {
                ShapeKind::Auto(a) => a.stroke.as_ref().map(|st| (sl.id, sh.id, st.clone())),
                ShapeKind::Line(st) => Some((sl.id, sh.id, st.clone())),
                _ => None,
            })
        })
        .unwrap_or_else(|| {
            let (sl_id, sh_id) = s
                .p()
                .slides
                .iter()
                .find_map(|sl| sl.shapes.iter().find(|sh| matches!(sh.kind, ShapeKind::Auto(_))).map(|sh| (sl.id, sh.id)))
                .expect("an auto shape");
            (sl_id, sh_id, Stroke::solid(Color::opaque(Rgb(0x70ad47)), 38_100))
        });

    // Change to Inset, save, and verify reread matches Inset
    stroke.alignment = LineAlignment::Inset;
    s.run(vec![Command::SetStroke { slide, shape, stroke: Some(stroke.clone()) }]);

    // Change back to Center, save, and verify reread matches Center (a:ln/@algn removed)
    stroke.alignment = LineAlignment::Center;
    s.run(vec![Command::SetStroke { slide, shape, stroke: Some(stroke) }]);
}

#[test]
fn gradient_numeric_boundaries() {
    let mut s=Scenario::open("gradient-numeric","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();let auto=AutoShape::new(Geometry::Rect,Fill::Solid(Color::BLACK),None);let saved=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto))}]);s.data=saved;s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].id;
    for (case,angle,positions) in [("angle-zero",0,vec![0,100_000]),("angle-max",21_599_999,vec![0,100_000]),("same-position",0,vec![50_000,50_000])] {
        let stops=positions.into_iter().enumerate().map(|(i,position)|ibis_doc_model::GradientStop {position,color:if i==0 {Color::BLACK} else {Color::WHITE}}).collect();let saved=s.run(vec![Command::SetFill {slide,shape,fill:Fill::Linear {angle,scaled:false,stops}}]);std::fs::write(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../target/pptx-edit/gradient-bound-{case}.pptx")),saved).unwrap();
    }
    for (angle,positions) in [(-1,vec![0,100_000]),(21_600_000,vec![0,100_000]),(0,vec![]),(0,vec![50_000]),(0,vec![0,100_001])] {
        let stops:Vec<_>=positions.into_iter().enumerate().map(|(i,position)|ibis_doc_model::GradientStop {position,color:if i==0 {Color::BLACK} else {Color::WHITE}}).collect();let mut fills=vec![Fill::Linear {angle,scaled:false,stops:stops.clone()}];if angle==0 {fills.push(Fill::Path {path:ibis_doc_model::GradientPath::Circle,focus:[50_000;4],stops});}
        for fill in fills {
            let mut candidate=s.p().slides[0].shapes[index].clone();let ShapeKind::Auto(a)=&mut candidate.kind else {panic!("auto")};a.fill=fill.clone();assert!(ibis_pptx::save::check_shape_insertion(&candidate).is_err());
            for command in [Command::SetFill {slide,shape,fill:fill.clone()},Command::SetBackground {slide,fill:fill.clone()}] {let mut preview=s.p().clone();preview.apply(command).unwrap();assert!(s.opened.source.save(&preview,SaveOptions::default()).is_err());}
        }
    }

}










#[test]
fn pattern_colour_edits_keep_unknown_properties() {
    let mut s=Scenario::open("pattern-colour-edit-unknown","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let auto_id=s.document.ids().new_shape_id();let table_id=s.document.ids().new_shape_id();let fill=Fill::Pattern {preset:"pct25".into(),foreground:Color {rgb:Rgb(0x112233),alpha:128},background:Color {rgb:Rgb(0xFEDCBA),alpha:64}};let mut table=ibis_doc_model::Table::new_grid(2,2,3_000_000,1_500_000);table.rows[0].cells[0].fill=fill.clone();let data=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(auto_id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(AutoShape::new(Geometry::Rect,fill.clone(),None)))},Command::InsertShape {slide,index:index+1,shape:Shape::new(table_id,EmuRect::new(3_500_000,500_000,3_000_000,1_500_000),ShapeKind::Table(Box::new(table)))},Command::SetBackground {slide,fill}]);
    let mut package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:pattern-colour-edit");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignore=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignore.trim());let patterns=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="pattFill").collect::<Vec<_>>();assert_eq!(patterns.len(),3);
    for (i,pattern) in patterns.into_iter().enumerate() {xml.set_attr(pattern,"fixture:target",&i.to_string());let payload=xml.append_element(pattern,"fixture:payload");xml.append_text(payload,"pattern root retained");for name in ["fgClr","bgClr"] {let container=xml.child(pattern,name).unwrap();xml.set_attr(container,"fixture:keep",name);let payload=xml.append_element(container,"fixture:payload");xml.append_text(payload,"container retained");let color=xml.child(container,"srgbClr").unwrap();xml.set_attr(color,"fixture:keep","colour");let payload=xml.append_element(color,"fixture:payload");xml.append_text(payload,"colour retained");let alpha=xml.child(color,"alpha").unwrap();xml.set_attr(alpha,"fixture:keep","alpha");for (name,value) in [("alphaMod","50000"),("alphaOff","10000")] {let node=xml.append_element(color,&format!("a:{name}"));xml.set_attr(node,"val",value);xml.set_attr(node,"fixture:keep",name);}let foreign=xml.append_element(color,"fixture:alpha");xml.set_attr(foreign,"val","0");let foreign=xml.insert_element(container,0,"fixture:srgbClr");xml.set_attr(foreign,"val","000000");}}
    s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let auto_id=s.p().slides[0].shapes[index].id;let table_id=s.p().slides[0].shapes[index+1].id;
    for (fg,bg) in [(Color {rgb:Rgb(0x445566),alpha:200},Color {rgb:Rgb(0x778899),alpha:40}),(Color::BLACK,Color::WHITE)] {let fill=Fill::Pattern {preset:"weave".into(),foreground:fg,background:bg};let ShapeKind::Table(table)=&s.p().slides[0].shapes[index+1].kind else {panic!("table")};let mut table=(**table).clone();table.rows[0].cells[0].fill=fill.clone();let data=s.run(vec![Command::SetFill {slide,shape:auto_id,fill:fill.clone()},Command::SetBackground {slide,fill},Command::SetKind {slide,shape:table_id,kind:Box::new(ShapeKind::Table(Box::new(table)))}]);let package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();for target in ["0","1","2"] {let pattern=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:target")==Some(target)).expect("pattern root retained after colour edit");assert_eq!(xml.text(xml.child(pattern,"payload").unwrap()),"pattern root retained");for (name,value) in [("fgClr",fg),("bgClr",bg)] {let container=xml.child(pattern,name).unwrap();assert_eq!(xml.attr(container,"fixture:keep"),Some(name));assert_eq!(xml.text(xml.child(container,"payload").unwrap()),"container retained");let color=xml.elements(container).find(|&n|xml.namespace(n)==Some("http://schemas.openxmlformats.org/drawingml/2006/main")&&xml.local_name(n)=="srgbClr").unwrap();assert_eq!(xml.attr(color,"val"),Some(format!("{:06X}",value.rgb.0).as_str()));assert_eq!(xml.attr(color,"fixture:keep"),Some("colour"));assert_eq!(xml.text(xml.child(color,"payload").unwrap()),"colour retained");let alpha=xml.elements(color).find(|&n|xml.namespace(n)==Some("http://schemas.openxmlformats.org/drawingml/2006/main")&&xml.local_name(n)=="alpha").unwrap();assert_eq!(xml.attr(alpha,"fixture:keep"),Some("alpha"));for (name,value) in [("alphaMod","100000"),("alphaOff","0")] {let node=xml.elements(color).find(|&n|xml.namespace(n)==Some("http://schemas.openxmlformats.org/drawingml/2006/main")&&xml.local_name(n)==name).unwrap();assert_eq!(xml.attr(node,"val"),Some(value));assert_eq!(xml.attr(node,"fixture:keep"),Some(name));}let foreign=xml.elements(color).find(|&n|xml.namespace(n)==Some("urn:ibis:test:pattern-colour-edit")&&xml.local_name(n)=="alpha").unwrap();assert_eq!(xml.attr(foreign,"val"),Some("0"));let foreign=xml.elements(container).find(|&n|xml.namespace(n)==Some("urn:ibis:test:pattern-colour-edit")&&xml.local_name(n)=="srgbClr").unwrap();assert_eq!(xml.attr(foreign,"val"),Some("000000"));}}}
}

#[test]
fn pattern_alpha_edit_keeps_rgb_transform_and_refuses_rgb_conversion() {
    let mut s=Scenario::open("pattern-alpha-transform","editing/pptx-plan.pptx");
    let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();
    let data=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(AutoShape::new(Geometry::Rect,Fill::Pattern {preset:"pct25".into(),foreground:Color::BLACK,background:Color::WHITE},None)))}]);
    let mut package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();
    let pattern=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="pattFill").last().unwrap();let color=xml.child(xml.child(pattern,"fgClr").unwrap(),"srgbClr").unwrap();let tint=xml.append_element(color,"a:tint");xml.set_attr(tint,"val","50000");
    s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());
    let slide=s.p().slides[0].id;let id=s.p().slides[0].shapes[index].id;let ShapeKind::Auto(a)=&s.p().slides[0].shapes[index].kind else {panic!("auto")};let Fill::Pattern {foreground,background,..}=a.fill else {panic!("pattern")};
    let mut changed=foreground;changed.alpha=123;let data=s.run(vec![Command::SetFill {slide,shape:id,fill:Fill::Pattern {preset:"weave".into(),foreground:changed,background}}]);
    let package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();let pattern=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="pattFill").last().unwrap();let color=xml.child(xml.child(pattern,"fgClr").unwrap(),"srgbClr").unwrap();assert_eq!(xml.attr(color,"val"),Some("000000"));assert_eq!(xml.attr(xml.child(color,"tint").unwrap(),"val"),Some("50000"));
    let mut preview=s.p().clone();changed.rgb=Rgb(0x123456);preview.apply(Command::SetFill {slide,shape:id,fill:Fill::Pattern {preset:"weave".into(),foreground:changed,background}}).unwrap();assert!(s.opened.source.save(&preview,SaveOptions::default()).is_err());
}

#[test]
fn pattern_colours_ignore_foreign_colour_nodes_attributes_and_modifiers() {
    for (kind,scenario) in [("colour","pattern-colour-namespace"),("attribute","pattern-colour-attribute"),("modifier","pattern-colour-modifier")] {
        let mut s=Scenario::open(scenario,"editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();let foreground=Color {rgb:Rgb(0x112233),alpha:128};let background=Color {rgb:Rgb(0xFEDCBA),alpha:64};let fill=Fill::Pattern {preset:"pct25".into(),foreground,background};let data=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(AutoShape::new(Geometry::Rect,fill.clone(),None)))}]);
        let mut package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:colour-namespace");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignore=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignore.trim());let pattern=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="pattFill").last().unwrap();xml.set_attr(pattern,"fixture:kind",kind);let fg=xml.child(pattern,"fgClr").unwrap();let color=xml.child(fg,"srgbClr").unwrap();
        match kind {"colour"=>{let foreign=xml.insert_element(fg,0,"fixture:srgbClr");xml.set_attr(foreign,"val","000000");},"attribute"=>{xml.remove_attr(color,"val");xml.set_attr(color,"fixture:val","000000");xml.set_attr(color,"val","112233");},"modifier"=>{let foreign=xml.append_element(color,"fixture:alpha");xml.set_attr(foreign,"val","0");},_=>unreachable!()}
        s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let ShapeKind::Auto(a)=&s.p().slides[0].shapes[index].kind else {panic!("auto")};assert_eq!(a.fill,fill,"foreign {kind} must not change known colour state");let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].id;let data=s.run(vec![Command::SetFill {slide,shape,fill:Fill::Pattern {preset:"diagCross".into(),foreground,background}}]);let package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();let pattern=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:kind")==Some(kind)).unwrap();let fg=xml.child(pattern,"fgClr").unwrap();let color=xml.elements(fg).find(|&n|xml.namespace(n)==Some("http://schemas.openxmlformats.org/drawingml/2006/main")&&xml.local_name(n)=="srgbClr").unwrap();assert_eq!(xml.attr(color,"val"),Some("112233"));match kind {"colour"=>{let foreign=xml.elements(fg).find(|&n|xml.namespace(n)==Some("urn:ibis:test:colour-namespace")).unwrap();assert_eq!(xml.attr(foreign,"val"),Some("000000"));},"attribute"=>assert_eq!(xml.attr(color,"fixture:val"),Some("000000")),"modifier"=>{let foreign=xml.elements(color).find(|&n|xml.namespace(n)==Some("urn:ibis:test:colour-namespace")).unwrap();assert_eq!(xml.attr(foreign,"val"),Some("0"));},_=>unreachable!()}
    }
}

#[test]
fn pattern_unknown_presets_are_rejected_before_saving() {
    let mut s=Scenario::open("pattern-invalid-preset","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();let fill=Fill::Pattern {preset:"pct25".into(),foreground:Color::BLACK,background:Color::WHITE};let mut table=ibis_doc_model::Table::new_grid(2,2,3_000_000,1_500_000);table.rows[0].cells[0].fill=fill.clone();let tid=s.document.ids().new_shape_id();let data=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(AutoShape::new(Geometry::Rect,fill.clone(),None)))},Command::InsertShape {slide,index:index+1,shape:Shape::new(tid,EmuRect::new(3_500_000,500_000,3_000_000,1_500_000),ShapeKind::Table(Box::new(table)))},Command::SetBackground {slide,fill}]);s.opened=ibis_pptx::open(data.clone()).unwrap();s.data=data;s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;
    for i in [index,index+1] {assert!(ibis_pptx::save::check_shape_insertion(&s.p().slides[0].shapes[i]).is_ok());}
    for preset in ["","futurePattern","pct25\"<&"] {
        let fill=Fill::Pattern {preset:preset.into(),foreground:Color::BLACK,background:Color::WHITE};let mut auto=s.p().slides[0].shapes[index].clone();let ShapeKind::Auto(a)=&mut auto.kind else {panic!("auto")};a.fill=fill.clone();assert!(ibis_pptx::save::check_shape_insertion(&auto).is_err());let mut preview=s.p().clone();preview.apply(Command::SetFill {slide,shape:auto.id,fill:fill.clone()}).unwrap();assert!(s.opened.source.save(&preview,SaveOptions::default()).is_err());let mut preview=s.p().clone();preview.apply(Command::SetBackground {slide,fill:fill.clone()}).unwrap();assert!(s.opened.source.save(&preview,SaveOptions::default()).is_err());let mut shape=s.p().slides[0].shapes[index+1].clone();let ShapeKind::Table(t)=&mut shape.kind else {panic!("table")};t.rows[0].cells[0].fill=fill;assert!(ibis_pptx::save::check_shape_insertion(&shape).is_err());let mut preview=s.p().clone();preview.apply(Command::SetKind {slide,shape:shape.id,kind:Box::new(shape.kind)}).unwrap();assert!(s.opened.source.save(&preview,SaveOptions::default()).is_err());
    }
}

#[test]
fn pattern_preset_edits_keep_unknown_properties_and_ignore_foreign_state() {
    let mut s=Scenario::open("pattern-preset-unknown","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let auto_id=s.document.ids().new_shape_id();let table_id=s.document.ids().new_shape_id();let foreground=Color {rgb:Rgb(0x12ABCD),alpha:128};let background=Color {rgb:Rgb(0xFEDCBA),alpha:64};let fill=Fill::Pattern {preset:"pct25".into(),foreground,background};let auto=AutoShape::new(Geometry::Rect,fill.clone(),None);let mut table=ibis_doc_model::Table::new_grid(2,2,3_000_000,1_500_000);table.rows[0].cells[0].fill=fill.clone();
    let data=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(auto_id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto))},Command::InsertShape {slide,index:index+1,shape:Shape::new(table_id,EmuRect::new(3_500_000,500_000,3_000_000,1_500_000),ShapeKind::Table(Box::new(table)))},Command::SetBackground {slide,fill:fill.clone()}]);let mut package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:pattern-preset");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignore=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignore.trim());let patterns=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="pattFill").collect::<Vec<_>>();assert_eq!(patterns.len(),3);
    for (i,pattern) in patterns.into_iter().enumerate() {xml.set_attr(pattern,"fixture:target",&i.to_string());xml.remove_attr(pattern,"prst");xml.set_attr(pattern,"fixture:prst","pct90");xml.set_attr(pattern,"prst","pct25");for name in ["fgClr","bgClr"] {let known=xml.child(pattern,name).unwrap();xml.set_attr(known,"fixture:keep",name);let color=xml.child(known,"srgbClr").unwrap();xml.set_attr(color,"fixture:keep","colour");let payload=xml.append_element(color,"fixture:payload");xml.append_text(payload,"pattern colour retained");let foreign=xml.insert_element(pattern,0,&format!("fixture:{name}"));xml.set_attr(foreign,"fixture:keep","foreign");let colour=xml.append_element(foreign,"a:srgbClr");xml.set_attr(colour,"val","777777");}let payload=xml.append_element(pattern,"fixture:payload");xml.append_text(payload,"pattern retained");}
    s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());assert_eq!(s.p().slides[0].background,fill,"foreign preset/colour state must not override known pattern");let ShapeKind::Auto(a)=&s.p().slides[0].shapes[index].kind else {panic!("auto")};assert_eq!(a.fill,fill);let slide=s.p().slides[0].id;let auto_id=s.p().slides[0].shapes[index].id;let table_id=s.p().slides[0].shapes[index+1].id;
    for preset in ["diagCross","pct5","zigZag"] {let fill=Fill::Pattern {preset:preset.into(),foreground,background};let ShapeKind::Table(table)=&s.p().slides[0].shapes[index+1].kind else {panic!("table")};let mut table=(**table).clone();table.rows[0].cells[0].fill=fill.clone();let saved=s.run(vec![Command::SetFill {slide,shape:auto_id,fill:fill.clone()},Command::SetBackground {slide,fill},Command::SetKind {slide,shape:table_id,kind:Box::new(ShapeKind::Table(Box::new(table)))}]);let package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();for target in ["0","1","2"] {let pattern=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:target")==Some(target)).expect("pattern root retained");assert_eq!(xml.attr(pattern,"fixture:prst"),Some("pct90"));assert_eq!(xml.attr(pattern,"prst"),Some(preset));assert_eq!(xml.text(xml.child(pattern,"payload").unwrap()),"pattern retained");for (name,rgb) in [("fgClr","12ABCD"),("bgClr","FEDCBA")] {let known=xml.elements(pattern).find(|&n|xml.namespace(n)==Some("http://schemas.openxmlformats.org/drawingml/2006/main")&&xml.local_name(n)==name).unwrap();assert_eq!(xml.attr(known,"fixture:keep"),Some(name));let color=xml.child(known,"srgbClr").unwrap();assert_eq!(xml.attr(color,"val"),Some(rgb));assert_eq!(xml.attr(color,"fixture:keep"),Some("colour"));assert_eq!(xml.text(xml.child(color,"payload").unwrap()),"pattern colour retained");let foreign=xml.elements(pattern).find(|&n|xml.namespace(n)==Some("urn:ibis:test:pattern-preset")&&xml.local_name(n)==name).unwrap();assert_eq!(xml.attr(foreign,"fixture:keep"),Some("foreign"));assert_eq!(xml.attr(xml.child(foreign,"srgbClr").unwrap(),"val"),Some("777777"));}}}
}

#[test]
fn path_gradient_adds_missing_focus_without_consuming_foreign_rect() {
    use ibis_doc_model::GradientPath;
    let mut s=Scenario::open("gradient-path-missing-focus","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();let stops=vec![ibis_doc_model::GradientStop {position:0,color:Color::BLACK},ibis_doc_model::GradientStop {position:100_000,color:Color::WHITE}];let fill=Fill::Path {path:GradientPath::Circle,focus:[0;4],stops:stops.clone()};
    let data=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(AutoShape::new(Geometry::Rect,fill.clone(),None)))}]);let mut package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:path-missing-focus");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignore=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignore.trim());let path=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="path").last().unwrap();let rect=xml.child(path,"fillToRect").unwrap();xml.detach(rect);xml.set_attr(path,"fixture:keep","missing-focus");let foreign=xml.append_element(path,"fixture:fillToRect");xml.set_attr(foreign,"l","9999");xml.append_text(foreign,"foreign focus retained");
    s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let ShapeKind::Auto(a)=&s.p().slides[0].shapes[index].kind else {panic!("auto")};assert_eq!(a.fill,fill);let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].id;
    for focus in [[0;4],[12_345,23_456,34_567,45_678]] {let data=s.run(vec![Command::SetFill {slide,shape,fill:Fill::Path {path:GradientPath::Rect,focus,stops:stops.clone()}}]);let package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();let path=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("missing-focus")).unwrap();let rect=xml.elements(path).find(|&n|xml.namespace(n)==Some("http://schemas.openxmlformats.org/drawingml/2006/main")&&xml.local_name(n)=="fillToRect");assert_eq!(rect.is_some(),focus!=[0;4]);if let Some(rect)=rect {for (key,value) in ["l","t","r","b"].into_iter().zip(focus) {assert_eq!(xml.attr(rect,key),Some(value.to_string().as_str()));}}let foreign=xml.elements(path).find(|&n|xml.namespace(n)==Some("urn:ibis:test:path-missing-focus")).unwrap();assert_eq!(xml.attr(foreign,"l"),Some("9999"));assert_eq!(xml.text(foreign),"foreign focus retained");}
}

#[test]
fn sections_create_rename_boundaries_and_clear_round_trip() {
    use ibis_doc_model::Section;
    const S:&str="http://schemas.microsoft.com/office/powerpoint/2010/main";
    let mut s=Scenario::open("sections-edit","editing/pptx-plan.pptx");let ids=s.p().slides.iter().map(|slide|slide.id).collect::<Vec<_>>();
    let mut sections=vec![Section {id:"{11111111-1111-1111-1111-111111111111}".into(),name:"first".into(),slides:ids[..2].to_vec()},Section {id:"{22222222-2222-2222-2222-222222222222}".into(),name:"second".into(),slides:ids[2..].to_vec()}];
    let data=s.run(vec![Command::SetSections {sections:sections.clone()}]);
    let mut clean=Scenario::open("sections-clear-clean","editing/pptx-plan.pptx");clean.data=data.clone();clean.opened=ibis_pptx::open(data.clone()).unwrap();clean.document=Document::new(clean.opened.presentation.clone());let cleared=clean.run(vec![Command::SetSections {sections:vec![]}]);std::fs::write(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/pptx-edit/sections-cleared.pptx"),cleared).unwrap();
    let mut package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml_mut("ppt/presentation.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:section-edit");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignore=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignore.trim());let list=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.namespace(n)==Some(S)&&xml.local_name(n)=="sectionLst").unwrap();xml.set_attr(list,"fixture:keep","list");let payload=xml.append_element(list,"fixture:payload");xml.append_text(payload,"section list kept");
    let nodes=xml.elements(list).filter(|&n|xml.namespace(n)==Some(S)&&xml.local_name(n)=="section").collect::<Vec<_>>();for (i,section) in nodes.into_iter().enumerate() {xml.set_attr(section,"fixture:old",&i.to_string());let members=xml.child(section,"sldIdLst").unwrap();xml.set_attr(members,"fixture:keep","members");let entries=xml.elements(members).collect::<Vec<_>>();for member in entries {let number=xml.attr(member,"id").unwrap().to_owned();xml.set_attr(member,"fixture:origin",&number);}}
    s.data=package.save(Default::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let ids=s.p().slides.iter().map(|slide|slide.id).collect::<Vec<_>>();sections=s.p().sections.clone();assert_eq!(sections.len(),2);sections[0].name="概要 & <A> \"日本語\"".into();sections[0].slides=ids[..1].to_vec();sections[1].slides=ids[1..].to_vec();sections.insert(1,Section {id:"{33333333-3333-3333-3333-333333333333}".into(),name:"empty".into(),slides:vec![]});
    let mut preview=s.p().clone();preview.apply(Command::SetSections {sections:vec![]}).unwrap();assert!(s.opened.source.save(&preview,Default::default()).is_err(),"opaque list metadata prevents destructive clear");assert_eq!(s.p(),&s.opened.presentation);
    for edited in [sections.clone(),vec![Section {slides:ids.clone(),name:"all slides".into(),..sections[2].clone()}]] {
        let data=s.run(vec![Command::SetSections {sections:edited.clone()}]);let package=ibis_ooxml::Package::open(data.clone()).unwrap();let xml=package.xml("ppt/presentation.xml").unwrap();let root=xml.root_element().unwrap();let list=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("list")).expect("section list metadata kept");assert_eq!(xml.text(xml.child(list,"payload").unwrap()),"section list kept");let nodes=xml.elements(list).filter(|&n|xml.namespace(n)==Some(S)&&xml.local_name(n)=="section").collect::<Vec<_>>();assert_eq!(nodes.len(),edited.len());for node in nodes {let id=xml.attr(node,"id").unwrap();if id=="{11111111-1111-1111-1111-111111111111}" {assert_eq!(xml.attr(node,"fixture:old"),Some("0"));}if id=="{22222222-2222-2222-2222-222222222222}" {assert_eq!(xml.attr(node,"fixture:old"),Some("1"));}let members=xml.child(node,"sldIdLst").unwrap();for member in xml.elements(members).filter(|&n|xml.namespace(n)==Some(S)) {assert_eq!(xml.attr(member,"fixture:origin"),xml.attr(member,"id"),"moving a slide keeps its original membership metadata");}}
        let tag=if edited.is_empty() {"cleared"} else if edited.len()==1 {"merged"} else {"renamed-empty"};std::fs::write(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../target/pptx-edit/sections-{tag}.pptx")),data).unwrap();
    }
    let reordered=vec![s.opened.presentation.sections[1].clone(),s.opened.presentation.sections[0].clone()];let data=s.run(vec![Command::Group(vec![Command::MoveSlide {slide:ids[0],index:3},Command::MoveSlide {slide:ids[1],index:3},Command::SetSections {sections:reordered.clone()}])]);let package=ibis_ooxml::Package::open(data.clone()).unwrap();let xml=package.xml("ppt/presentation.xml").unwrap();let root=xml.root_element().unwrap();let list=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("list")).unwrap();assert_eq!(xml.text(xml.child(list,"payload").unwrap()),"section list kept");let sections=xml.elements(list).filter(|&n|xml.namespace(n)==Some(S)&&xml.local_name(n)=="section").collect::<Vec<_>>();for (node,expected) in sections.into_iter().zip(reordered) {assert_eq!(xml.attr(node,"id"),Some(expected.id.as_str()));assert!(xml.attr(node,"fixture:old").is_some());let members=xml.child(node,"sldIdLst").unwrap();assert_eq!(xml.attr(members,"fixture:keep"),Some("members"));for node in xml.elements(members).filter(|&n|xml.namespace(n)==Some(S)) {assert_eq!(xml.attr(node,"fixture:origin"),xml.attr(node,"id"));}}
    std::fs::write(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/pptx-edit/sections-moved-opaque.pptx"),data).unwrap();
}

#[test]
fn slide_reorder_keeps_foreign_section_nodes_and_updates_known_members() {
    const P:&str="http://schemas.openxmlformats.org/presentationml/2006/main";
    const S:&str="http://schemas.microsoft.com/office/powerpoint/2010/main";
    const NS:&str="urn:ibis:test:sections";
    for (kind,scenario) in [("extLst","sections-foreign-ext"),("sectionLst","sections-foreign-list"),("section","sections-foreign-section"),("sldIdLst","sections-foreign-ids"),("sldId","sections-foreign-id"),("scope","sections-prefix-scope")] {
        let mut s=Scenario::open(scenario,"editing/pptx-plan.pptx");let mut package=ibis_ooxml::Package::open(s.data.clone()).unwrap();let xml=package.xml_mut("ppt/presentation.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:p14",S);xml.set_attr(root,"xmlns:fixture",NS);xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignore=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignore.trim());let slide_list=xml.elements(root).find(|&n|xml.namespace(n)==Some(P)&&xml.local_name(n)=="sldIdLst").unwrap();let numbers=xml.elements(slide_list).map(|n|xml.attr(n,"id").unwrap().to_owned()).collect::<Vec<_>>();assert_eq!(numbers.len(),4);
        let ext=if let Some(n)=xml.elements(root).find(|&n|xml.namespace(n)==Some(P)&&xml.local_name(n)=="extLst") {n} else {xml.append_element(root,"p:extLst")};let extension=xml.append_element(ext,"p:ext");xml.set_attr(extension,"uri","{521415D9-36F7-43E2-AB2F-B90AF26B5E84}");let list=xml.append_element(extension,"p14:sectionLst");let mut sections=Vec::new();let mut lists=Vec::new();
        for i in 0..2 {let section=xml.append_element(list,"p14:section");xml.set_attr(section,"name",if i==0 {"first"} else {"second"});xml.set_attr(section,"id",if i==0 {"{11111111-1111-1111-1111-111111111111}"} else {"{22222222-2222-2222-2222-222222222222}"});xml.set_attr(section,"fixture:known",&i.to_string());let ids=xml.append_element(section,"p14:sldIdLst");for number in &numbers[i*2..i*2+2] {let id=xml.append_element(ids,"p14:sldId");xml.set_attr(id,"id",number);}sections.push(section);lists.push(ids);}
        if kind=="scope" {xml.set_attr(sections[0],"xmlns:local",S);xml.set_attr(sections[0],"xmlns:scoped",NS);xml.set_attr(sections[1],"xmlns:local",NS);xml.set_attr(sections[1],"xmlns:scoped","urn:ibis:test:other-section");let nodes=xml.elements(lists[0]).collect::<Vec<_>>();for node in nodes {xml.rename(node,"local:sldId");xml.set_attr(node,"scoped:token","opaque original binding");}}
        let parent=match kind {"extLst"=>root,"sectionLst"=>extension,"section"=>list,"sldIdLst"=>sections[0],"sldId"|"scope"=>lists[0],_=>unreachable!()};let foreign=xml.insert_element(parent,0,&format!("fixture:{}",if kind=="scope" {"sldId"} else {kind}));xml.set_attr(foreign,"fixture:keep",kind);let mut node=foreign;let descend=match kind {"extLst"=>&["sectionLst","section","sldIdLst","sldId"][..],"sectionLst"=>&["section","sldIdLst","sldId"][..],"section"=>&["sldIdLst","sldId"][..],"sldIdLst"=>&["sldId"][..],"sldId"|"scope"=>&[][..],_=>unreachable!()};for name in descend {node=xml.append_element(node,&format!("fixture:{name}"));}xml.set_attr(node,"id","99999");let payload=xml.append_element(foreign,"fixture:payload");xml.append_text(payload,"foreign section unchanged");
        s.data=package.save(Default::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let data=s.run(vec![Command::MoveSlide {slide,index:3}]);let package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml("ppt/presentation.xml").unwrap();let root=xml.root_element().unwrap();let foreign=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some(kind)).unwrap();let fake=std::iter::once(foreign).chain(xml.descendants(foreign)).find(|&n|xml.is_element(n)&&xml.namespace(n)==Some(NS)&&xml.local_name(n)=="sldId").expect("foreign section slide id survives");assert_eq!(xml.attr(fake,"id"),Some("99999"));assert_eq!(xml.text(xml.child(foreign,"payload").unwrap()),"foreign section unchanged");
        for (i,expected) in [(0,vec![numbers[1].as_str()]),(1,vec![numbers[2].as_str(),numbers[3].as_str(),numbers[0].as_str()])] {let section=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:known")==Some(i.to_string().as_str())).unwrap();let ids=xml.elements(section).find(|&n|xml.namespace(n)==Some(S)&&xml.local_name(n)=="sldIdLst").unwrap();let actual=xml.elements(ids).filter(|&n|xml.namespace(n)==Some(S)&&xml.local_name(n)=="sldId").map(|n|xml.attr(n,"id").unwrap()).collect::<Vec<_>>();assert_eq!(actual,expected,"known section members follow reorder, foreign {kind}");if kind=="scope" {for node in xml.elements(ids).filter(|&n|xml.attr(n,"scoped:token").is_some()) {assert_eq!(xml.resolve_prefix(node,Some("scoped")),Some(NS),"foreign attribute namespace binding follows moved id");}}}
    }
}

#[test]
fn gradient_type_changes_keep_root_stops_and_geometry_attributes() {
    use ibis_doc_model::{GradientPath,GradientStop};
    const A:&str="http://schemas.openxmlformats.org/drawingml/2006/main";
    for (old_path,scenario) in [(false,"gradient-type-linear-to-path"),(true,"gradient-type-path-to-linear")] {
        let stops=vec![GradientStop {position:0,color:Color {rgb:Rgb(0x112233),alpha:128}},GradientStop {position:100_000,color:Color::WHITE}];
        let fill_of=|path:bool| if path {Fill::Path {path:GradientPath::Shape,focus:[12_345,23_456,34_567,45_678],stops:stops.clone()}} else {Fill::Linear {angle:7_407_360,scaled:false,stops:stops.clone()}};
        let mut s=Scenario::open(scenario,"editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let auto_id=s.document.ids().new_shape_id();let table_id=s.document.ids().new_shape_id();let fill=fill_of(old_path);let mut table=ibis_doc_model::Table::new_grid(2,2,3_000_000,1_500_000);table.rows[0].cells[0].fill=fill.clone();
        let data=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(auto_id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(AutoShape::new(Geometry::Rect,fill.clone(),None)))},Command::InsertShape {slide,index:index+1,shape:Shape::new(table_id,EmuRect::new(3_500_000,500_000,3_000_000,1_500_000),ShapeKind::Table(Box::new(table)))},Command::SetBackground {slide,fill}]);
        let mut package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:gradient-type");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignore=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignore.trim());let grads=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.namespace(n)==Some(A)&&xml.local_name(n)=="gradFill").collect::<Vec<_>>();assert_eq!(grads.len(),3);
        for (target,grad) in grads.into_iter().enumerate() {xml.set_attr(grad,"fixture:target",&target.to_string());xml.set_attr(grad,"rotWithShape","0");let list=xml.child(grad,"gsLst").unwrap();xml.set_attr(list,"fixture:keep","list");let nodes=xml.elements(list).collect::<Vec<_>>();for (i,node) in nodes.into_iter().enumerate() {xml.set_attr(node,"fixture:index",&i.to_string());let color=xml.child(node,"srgbClr").unwrap();xml.set_attr(color,"fixture:keep","colour");let payload=xml.append_element(color,"fixture:payload");xml.append_text(payload,"type change stop retained");}let geometry=xml.child(grad,if old_path {"path"} else {"lin"}).unwrap();xml.set_attr(geometry,"fixture:keep","geometry");xml.set_attr(geometry,"fixture:ang","foreign angle");if old_path {let focus=xml.child(geometry,"fillToRect").unwrap();xml.append_text(focus," \n ");}}
        s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let auto_id=s.p().slides[0].shapes[index].id;let table_id=s.p().slides[0].shapes[index+1].id;let fill=fill_of(!old_path);let ShapeKind::Table(table)=&s.p().slides[0].shapes[index+1].kind else {panic!("table")};let mut table=(**table).clone();table.rows[0].cells[0].fill=fill.clone();let data=s.run(vec![Command::SetFill {slide,shape:auto_id,fill:fill.clone()},Command::SetBackground {slide,fill},Command::SetKind {slide,shape:table_id,kind:Box::new(ShapeKind::Table(Box::new(table)))}]);
        let package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();for target in ["0","1","2"] {
            let grad=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:target")==Some(target)).expect("gradient root retained after type change");assert_eq!(xml.attr(grad,"rotWithShape"),Some("0"));let list=xml.child(grad,"gsLst").unwrap();assert_eq!(xml.attr(list,"fixture:keep"),Some("list"));for (i,node) in xml.elements(list).enumerate() {assert_eq!(xml.attr(node,"fixture:index"),Some(i.to_string().as_str()));let color=xml.child(node,"srgbClr").unwrap();assert_eq!(xml.attr(color,"fixture:keep"),Some("colour"));assert_eq!(xml.text(xml.child(color,"payload").unwrap()),"type change stop retained");}let geometry=xml.elements(grad).find(|&n|xml.namespace(n)==Some(A)&&xml.local_name(n)==if old_path {"lin"} else {"path"}).unwrap();assert_eq!(xml.attr(geometry,"fixture:keep"),Some("geometry"));assert_eq!(xml.attr(geometry,"fixture:ang"),Some("foreign angle"));
        }
        if old_path {
            let mut package=ibis_ooxml::Package::open(s.data.clone()).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();let nodes=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.namespace(n)==Some(A)&&xml.local_name(n)=="fillToRect").collect::<Vec<_>>();assert_eq!(nodes.len(),3);for node in nodes {xml.set_attr(node,"fixture:focus","must not be silently lost");}
            let opened=ibis_pptx::open(package.save(Default::default()).unwrap()).unwrap();let slide=opened.presentation.slides[0].id;let auto_id=opened.presentation.slides[0].shapes[index].id;let table_id=opened.presentation.slides[0].shapes[index+1].id;let fill=fill_of(false);let ShapeKind::Table(table)=&opened.presentation.slides[0].shapes[index+1].kind else {panic!("table")};let mut table=(**table).clone();table.rows[0].cells[0].fill=fill.clone();
            for command in [Command::SetFill {slide,shape:auto_id,fill:fill.clone()},Command::SetBackground {slide,fill},Command::SetKind {slide,shape:table_id,kind:Box::new(ShapeKind::Table(Box::new(table)))}] {let mut preview=opened.presentation.clone();preview.apply(command).unwrap();assert!(opened.source.save(&preview,Default::default()).is_err(),"unknown focus must not be discarded");}
        }
    }
}

#[test]
fn gradient_stop_count_edits_keep_survivors_and_foreign_children() {
    use ibis_doc_model::{GradientPath,GradientStop};
    const A:&str="http://schemas.openxmlformats.org/drawingml/2006/main";
    const NS:&str="urn:ibis:test:gradient-stop-count";
    for (path_mode,scenario) in [(false,"gradient-stop-count-linear"),(true,"gradient-stop-count-path")] {
        let fill_of=|stops:Vec<GradientStop>| if path_mode {Fill::Path {path:GradientPath::Shape,focus:[10_000,20_000,30_000,40_000],stops}} else {Fill::Linear {angle:7_407_360,scaled:false,stops}};
        let old_stops=vec![GradientStop {position:0,color:Color {rgb:Rgb(0x112233),alpha:128}},GradientStop {position:50_000,color:Color {rgb:Rgb(0x445566),alpha:64}},GradientStop {position:100_000,color:Color::WHITE}];
        let mut s=Scenario::open(scenario,"editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let auto_id=s.document.ids().new_shape_id();let table_id=s.document.ids().new_shape_id();let fill=fill_of(old_stops.clone());let mut table=ibis_doc_model::Table::new_grid(2,2,3_000_000,1_500_000);table.rows[0].cells[0].fill=fill.clone();
        let data=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(auto_id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(AutoShape::new(Geometry::Rect,fill.clone(),None)))},Command::InsertShape {slide,index:index+1,shape:Shape::new(table_id,EmuRect::new(3_500_000,500_000,3_000_000,1_500_000),ShapeKind::Table(Box::new(table)))},Command::SetBackground {slide,fill}]);
        let mut package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture",NS);xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignore=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignore.trim());let grads=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.namespace(n)==Some(A)&&xml.local_name(n)=="gradFill").collect::<Vec<_>>();assert_eq!(grads.len(),3);
        for (target,grad) in grads.into_iter().enumerate() {
            xml.set_attr(grad,"fixture:target",&target.to_string());xml.set_attr(grad,"rotWithShape","0");let list=xml.child(grad,"gsLst").unwrap();xml.set_attr(list,"fixture:keep","list");let nodes=xml.elements(list).collect::<Vec<_>>();
            for (i,node) in nodes.iter().copied().enumerate() {xml.set_attr(node,"fixture:index",&i.to_string());let color=xml.child(node,"srgbClr").unwrap();xml.set_attr(color,"fixture:index",&i.to_string());let payload=xml.append_element(color,"fixture:payload");xml.append_text(payload,"surviving stop colour");}
            for node in nodes {xml.insert(list,0,node);}let foreign=xml.insert_element(list,1,"fixture:gs");xml.set_attr(foreign,"pos","12345");let payload=xml.append_element(foreign,"fixture:payload");xml.append_text(payload,"foreign stop sibling");let geometry=xml.child(grad,if path_mode {"path"} else {"lin"}).unwrap();xml.set_attr(geometry,"fixture:keep","geometry");
        }
        s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let auto_id=s.p().slides[0].shapes[index].id;let table_id=s.p().slides[0].shapes[index+1].id;
        let mut edited_first=old_stops[0].clone();edited_first.color.alpha=200;let mut edited_middle=old_stops[1].clone();edited_middle.color.rgb=Rgb(0x778899);
        for (stops,survivors) in [(vec![edited_first,GradientStop {position:25_000,color:Color::opaque(Rgb(0xAABBCC))},edited_middle,old_stops[2].clone()],vec!["0","1","2"]),(vec![old_stops[0].clone(),old_stops[2].clone()],vec!["0","2"])] {
            let fill=fill_of(stops.clone());let ShapeKind::Table(table)=&s.p().slides[0].shapes[index+1].kind else {panic!("table")};let mut table=(**table).clone();table.rows[0].cells[0].fill=fill.clone();let data=s.run(vec![Command::SetFill {slide,shape:auto_id,fill:fill.clone()},Command::SetBackground {slide,fill},Command::SetKind {slide,shape:table_id,kind:Box::new(ShapeKind::Table(Box::new(table)))}]);
            let package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();
            for target in ["0","1","2"] {
                let grad=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:target")==Some(target)).expect("gradient root retained after stop count edit");assert_eq!(xml.attr(grad,"rotWithShape"),Some("0"));let list=xml.child(grad,"gsLst").unwrap();assert_eq!(xml.attr(list,"fixture:keep"),Some("list"));let nodes=xml.elements(list).filter(|&n|xml.namespace(n)==Some(A)&&xml.local_name(n)=="gs").collect::<Vec<_>>();assert_eq!(nodes.len(),stops.len());let mut markers=nodes.iter().filter_map(|&n|xml.attr(n,"fixture:index")).collect::<Vec<_>>();markers.sort();assert_eq!(markers,survivors);
                for node in nodes {if let Some(original)=xml.attr(node,"fixture:index") {let color=xml.elements(node).find(|&n|xml.namespace(n)==Some(A)&&xml.local_name(n)=="srgbClr").unwrap();assert_eq!(xml.attr(color,"fixture:index"),Some(original));assert_eq!(xml.text(xml.child(color,"payload").unwrap()),"surviving stop colour");}}
                let foreign=xml.elements(list).find(|&n|xml.namespace(n)==Some(NS)&&xml.local_name(n)=="gs").expect("foreign sibling retained");assert_eq!(xml.attr(foreign,"pos"),Some("12345"));assert_eq!(xml.text(xml.child(foreign,"payload").unwrap()),"foreign stop sibling");assert_eq!(xml.attr(xml.child(grad,if path_mode {"path"} else {"lin"}).unwrap(),"fixture:keep"),Some("geometry"));
            }
        }
    }
}

#[test]
fn gradient_colour_edits_keep_original_stop_nodes() {
    use ibis_doc_model::{GradientPath,GradientStop};
    const A:&str="http://schemas.openxmlformats.org/drawingml/2006/main";
    const NS:&str="urn:ibis:test:gradient-colour-edit";
    for (path_mode,scenario) in [(false,"gradient-colour-linear"),(true,"gradient-colour-path")] {
        let fill_of=|stops:Vec<GradientStop>,step:usize| if path_mode {Fill::Path {path:if step==0 {GradientPath::Circle} else {GradientPath::Rect},focus:if step==0 {[50_000;4]} else {[12_345,23_456,34_567,45_678]},stops}} else {Fill::Linear {angle:if step==0 {0} else {7_407_360},scaled:step>0,stops}};
        let old_stops=vec![GradientStop {position:0,color:Color {rgb:Rgb(0x112233),alpha:128}},GradientStop {position:50_000,color:Color::opaque(Rgb(0x445566))},GradientStop {position:100_000,color:Color {rgb:Rgb(0x778899),alpha:64}}];
        let mut s=Scenario::open(scenario,"editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let auto_id=s.document.ids().new_shape_id();let table_id=s.document.ids().new_shape_id();let fill=fill_of(old_stops.clone(),0);let mut table=ibis_doc_model::Table::new_grid(2,2,3_000_000,1_500_000);table.rows[0].cells[0].fill=fill.clone();
        let data=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(auto_id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(AutoShape::new(Geometry::Rect,fill.clone(),None)))},Command::InsertShape {slide,index:index+1,shape:Shape::new(table_id,EmuRect::new(3_500_000,500_000,3_000_000,1_500_000),ShapeKind::Table(Box::new(table)))},Command::SetBackground {slide,fill}]);
        let mut package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture",NS);xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignore=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignore.trim());let grads=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.namespace(n)==Some(A)&&xml.local_name(n)=="gradFill").collect::<Vec<_>>();assert_eq!(grads.len(),3);
        for (target,grad) in grads.into_iter().enumerate() {
            xml.set_attr(grad,"fixture:target",&target.to_string());xml.set_attr(grad,"rotWithShape","0");let payload=xml.append_element(grad,"fixture:payload");xml.append_text(payload,"gradient colour retained");
            let geometry=xml.child(grad,if path_mode {"path"} else {"lin"}).unwrap();xml.set_attr(geometry,"fixture:keep","geometry");if !path_mode {xml.set_attr(geometry,"scaled","1");}
            let list=xml.child(grad,"gsLst").unwrap();xml.set_attr(list,"fixture:keep","list");let nodes=xml.elements(list).filter(|&n|xml.namespace(n)==Some(A)).collect::<Vec<_>>();
            for (i,node) in nodes.iter().copied().enumerate() {
                xml.set_attr(node,"fixture:index",&i.to_string());let color=xml.child(node,"srgbClr").unwrap();xml.set_attr(color,"fixture:keep","colour");let payload=xml.append_element(color,"fixture:payload");xml.append_text(payload,"stop colour retained");let foreign=xml.append_element(color,"fixture:alpha");xml.set_attr(foreign,"val","0");if let Some(alpha)=xml.child(color,"alpha") {xml.set_attr(alpha,"fixture:keep","alpha");}
                let foreign=xml.insert_element(node,0,"fixture:srgbClr");xml.set_attr(foreign,"val","000000");
            }
            for node in nodes {xml.insert(list,0,node);}let between=xml.insert_element(list,1,"fixture:between");xml.append_text(between,"between stops");
        }
        s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let auto_id=s.p().slides[0].shapes[index].id;let table_id=s.p().slides[0].shapes[index+1].id;
        for (step,positions) in [(1,[50_000,50_000,90_000]),(2,[10_000,50_000,99_999])] {
            let colours=[Color {rgb:Rgb(0x112244),alpha:200},Color {rgb:Rgb(0x445566),alpha:123},Color::opaque(Rgb(0x8899AA))];let stops=positions.into_iter().zip(colours).map(|(position,color)|GradientStop {position,color}).collect();let fill=fill_of(stops,step);let ShapeKind::Table(table)=&s.p().slides[0].shapes[index+1].kind else {panic!("table")};let mut table=(**table).clone();table.rows[0].cells[0].fill=fill.clone();
            let data=s.run(vec![Command::SetFill {slide,shape:auto_id,fill:fill.clone()},Command::SetBackground {slide,fill},Command::SetKind {slide,shape:table_id,kind:Box::new(ShapeKind::Table(Box::new(table)))}]);
            let package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();
            for target in ["0","1","2"] {
                let grad=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:target")==Some(target)).expect("original gradient root retained after colour edit");assert_eq!(xml.attr(grad,"rotWithShape"),Some("0"));assert_eq!(xml.text(xml.child(grad,"payload").unwrap()),"gradient colour retained");let geometry=xml.child(grad,if path_mode {"path"} else {"lin"}).unwrap();assert_eq!(xml.attr(geometry,"fixture:keep"),Some("geometry"));if !path_mode {assert_eq!(xml.attr(geometry,"scaled"),Some("1"));}
                let list=xml.child(grad,"gsLst").unwrap();assert_eq!(xml.attr(list,"fixture:keep"),Some("list"));let children=xml.elements(list).collect::<Vec<_>>();assert_eq!(xml.namespace(children[1]),Some(NS));assert_eq!(xml.text(children[1]),"between stops");
                let physical=children.into_iter().filter(|&n|xml.namespace(n)==Some(A)).collect::<Vec<_>>();let order=if step==1 {["2","0","1"]} else {["2","1","0"]};for (node,original) in physical.into_iter().zip(order) {
                    assert_eq!(xml.attr(node,"fixture:index"),Some(original));let i=original.parse::<usize>().unwrap();assert_eq!(xml.attr(node,"pos"),Some(positions[i].to_string().as_str()));let colour=xml.elements(node).find(|&n|xml.namespace(n)==Some(A)&&xml.local_name(n)=="srgbClr").unwrap();assert_eq!(xml.attr(colour,"val"),Some(format!("{:06X}",colours[i].rgb.0).as_str()));assert_eq!(xml.attr(colour,"fixture:keep"),Some("colour"));assert_eq!(xml.text(xml.child(colour,"payload").unwrap()),"stop colour retained");let foreign=xml.elements(node).find(|&n|xml.namespace(n)==Some(NS)).unwrap();assert_eq!(xml.attr(foreign,"val"),Some("000000"));let foreign=xml.elements(colour).find(|&n|xml.namespace(n)==Some(NS)&&xml.local_name(n)=="alpha").unwrap();assert_eq!(xml.attr(foreign,"val"),Some("0"));if i!=1 {let alpha=xml.elements(colour).find(|&n|xml.namespace(n)==Some(A)&&xml.local_name(n)=="alpha").unwrap();assert_eq!(xml.attr(alpha,"fixture:keep"),Some("alpha"));}
                }
            }
        }
    }
}

#[test]
fn path_gradient_geometry_keeps_unknown_properties() {
    use ibis_doc_model::GradientPath;
    let mut s=Scenario::open("gradient-path-geometry","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let auto_id=s.document.ids().new_shape_id();let table_id=s.document.ids().new_shape_id();
    let stops=vec![ibis_doc_model::GradientStop {position:0,color:Color::BLACK},ibis_doc_model::GradientStop {position:100_000,color:Color::WHITE}];let fill=Fill::Path {path:GradientPath::Circle,focus:[50_000;4],stops:stops.clone()};let auto=AutoShape::new(Geometry::Rect,fill.clone(),None);let mut table=ibis_doc_model::Table::new_grid(2,2,3_000_000,1_500_000);table.rows[0].cells[0].fill=fill.clone();
    let data=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(auto_id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto))},Command::InsertShape {slide,index:index+1,shape:Shape::new(table_id,EmuRect::new(3_500_000,500_000,3_000_000,1_500_000),ShapeKind::Table(Box::new(table)))},Command::SetBackground {slide,fill}]);
    let mut package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:path-geometry");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignore=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignore.trim());
    let grads=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="gradFill").collect::<Vec<_>>();assert_eq!(grads.len(),3);
    for (i,grad) in grads.into_iter().enumerate() {xml.set_attr(grad,"fixture:target",&i.to_string());xml.set_attr(grad,"rotWithShape","0");let path=xml.child(grad,"path").unwrap();xml.set_attr(path,"fixture:keep","path");let rect=xml.child(path,"fillToRect").unwrap();xml.set_attr(rect,"fixture:keep","focus");let payload=xml.append_element(path,"fixture:payload");xml.append_text(payload,"opaque path");let list=xml.child(grad,"gsLst").unwrap();xml.set_attr(list,"fixture:keep","list");let nodes=xml.elements(list).collect::<Vec<_>>();for (i,node) in nodes.into_iter().enumerate() {xml.set_attr(node,"fixture:index",&i.to_string());let color=xml.child(node,"srgbClr").unwrap();xml.set_attr(color,"fixture:keep","color");let payload=xml.append_element(color,"fixture:payload");xml.append_text(payload,"opaque colour");}let payload=xml.append_element(grad,"fixture:payload");xml.append_text(payload,"opaque gradient");}
    s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let auto_id=s.p().slides[0].shapes[index].id;let table_id=s.p().slides[0].shapes[index+1].id;
    for (path,focus) in [(GradientPath::Rect,[12_345,23_456,34_567,45_678]),(GradientPath::Shape,[0,0,100_000,100_000]),(GradientPath::Circle,[50_000;4])] {
        let fill=Fill::Path {path,focus,stops:stops.clone()};let ShapeKind::Table(table)=&s.p().slides[0].shapes[index+1].kind else {panic!("table")};let mut table=(**table).clone();table.rows[0].cells[0].fill=fill.clone();
        let saved=s.run(vec![Command::SetFill {slide,shape:auto_id,fill:fill.clone()},Command::SetBackground {slide,fill},Command::SetKind {slide,shape:table_id,kind:Box::new(ShapeKind::Table(Box::new(table)))}]);let package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();
        for target in ["0","1","2"] {let grad=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:target")==Some(target)).expect("path gradient attributes retained");assert_eq!(xml.attr(grad,"rotWithShape"),Some("0"));assert_eq!(xml.text(xml.child(grad,"payload").unwrap()),"opaque gradient");let node=xml.child(grad,"path").unwrap();assert_eq!(xml.attr(node,"path"),Some(path.name()));assert_eq!(xml.attr(node,"fixture:keep"),Some("path"));assert_eq!(xml.text(xml.child(node,"payload").unwrap()),"opaque path");let rect=xml.child(node,"fillToRect").unwrap();assert_eq!(xml.attr(rect,"fixture:keep"),Some("focus"));for (key,value) in ["l","t","r","b"].into_iter().zip(focus) {assert_eq!(xml.attr(rect,key),Some(value.to_string().as_str()));}let list=xml.child(grad,"gsLst").unwrap();assert_eq!(xml.attr(list,"fixture:keep"),Some("list"));for (i,node) in xml.elements(list).enumerate() {assert_eq!(xml.attr(node,"fixture:index"),Some(i.to_string().as_str()));let color=xml.child(node,"srgbClr").unwrap();assert_eq!(xml.attr(color,"fixture:keep"),Some("color"));assert_eq!(xml.text(xml.child(color,"payload").unwrap()),"opaque colour");}}
    }
}

#[test]
fn background_and_cell_gradient_geometry_keeps_unknown_properties() {
    let mut s=Scenario::open("gradient-background-cell-geometry","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();
    let stops=vec![ibis_doc_model::GradientStop {position:0,color:Color::BLACK},ibis_doc_model::GradientStop {position:100_000,color:Color::WHITE}];let fill=Fill::Linear {angle:0,scaled:false,stops:stops.clone()};let mut table=ibis_doc_model::Table::new_grid(2,2,3_000_000,1_500_000);table.rows[0].cells[0].fill=fill.clone();
    let data=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(id,EmuRect::new(500_000,500_000,3_000_000,1_500_000),ShapeKind::Table(Box::new(table)))},Command::SetBackground {slide,fill}]);
    let mut package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:gradient-targets");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignore=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignore.trim());
    let grads=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="gradFill").collect::<Vec<_>>();assert_eq!(grads.len(),2);
    for (i,grad) in grads.into_iter().enumerate() {xml.set_attr(grad,"fixture:target",&i.to_string());xml.set_attr(grad,"rotWithShape","0");let lin=xml.child(grad,"lin").unwrap();xml.set_attr(lin,"scaled","1");xml.set_attr(lin,"fixture:keep","line");let list=xml.child(grad,"gsLst").unwrap();xml.set_attr(list,"fixture:keep","list");let nodes=xml.elements(list).collect::<Vec<_>>();for (i,node) in nodes.into_iter().enumerate() {xml.set_attr(node,"fixture:index",&i.to_string());let color=xml.child(node,"srgbClr").unwrap();xml.set_attr(color,"fixture:keep","color");let payload=xml.append_element(color,"fixture:payload");xml.append_text(payload,"colour opaque payload");}let payload=xml.append_element(grad,"fixture:payload");xml.append_text(payload,"target gradient payload");}
    s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].id;
    for (angle,positions) in [(7_407_360,[10_000,90_000]),(2_700_000,[0,100_000]),(5_400_000,[50_000,50_000])] {
        let mut stops=stops.clone();for (stop,position) in stops.iter_mut().zip(positions) {stop.position=position;}let fill=Fill::Linear {angle,scaled:true,stops};let ShapeKind::Table(table)=&s.p().slides[0].shapes[index].kind else {panic!("table")};let mut table=(**table).clone();table.rows[0].cells[0].fill=fill.clone();
        let saved=s.run(vec![Command::SetBackground {slide,fill},Command::SetKind {slide,shape,kind:Box::new(ShapeKind::Table(Box::new(table)))}]);let package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();
        for target in ["0","1"] {let grad=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:target")==Some(target)).expect("background/cell gradient attributes retained");assert_eq!(xml.attr(grad,"rotWithShape"),Some("0"));assert_eq!(xml.text(xml.child(grad,"payload").unwrap()),"target gradient payload");let lin=xml.child(grad,"lin").unwrap();assert_eq!(xml.attr(lin,"scaled"),Some("1"));assert_eq!(xml.attr(lin,"fixture:keep"),Some("line"));let list=xml.child(grad,"gsLst").unwrap();assert_eq!(xml.attr(list,"fixture:keep"),Some("list"));for (i,node) in xml.elements(list).enumerate() {assert_eq!(xml.attr(node,"fixture:index"),Some(i.to_string().as_str()));assert_eq!(xml.attr(node,"pos"),Some(positions[i].to_string().as_str()));let color=xml.child(node,"srgbClr").unwrap();assert_eq!(xml.attr(color,"fixture:keep"),Some("color"));assert_eq!(xml.attr(color,"val"),Some(["000000","FFFFFF"][i]));assert_eq!(xml.text(xml.child(color,"payload").unwrap()),"colour opaque payload");}}
    }
}

#[test]
fn gradient_reader_ignores_foreign_geometry_attributes() {
    for (is_path,scenario) in [(false,"gradient-attrs-linear"),(true,"gradient-attrs-path")] {
        let mut s=Scenario::open(scenario,"editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();
        let stops=vec![ibis_doc_model::GradientStop {position:0,color:Color::BLACK},ibis_doc_model::GradientStop {position:100_000,color:Color::WHITE}];
        let fill=if is_path {Fill::Path {path:ibis_doc_model::GradientPath::Circle,focus:[50_000;4],stops}} else {Fill::Linear {angle:0,scaled:false,stops}};
        let auto=AutoShape::new(Geometry::Rect,fill.clone(),None);let data=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto))}]);
        let mut package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();
        xml.set_attr(root,"xmlns:fixture","urn:ibis:test:gradient-attrs");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignore=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignore.trim());
        let grad=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="gradFill").last().unwrap();let list=xml.child(grad,"gsLst").unwrap();let first=xml.elements(list).next().unwrap();
        let mut attrs=vec![(first,"pos","12345")];
        if is_path {let path=xml.child(grad,"path").unwrap();let rect=xml.child(path,"fillToRect").unwrap();attrs.push((path,"path","rect"));for key in ["l","t","r","b"] {attrs.push((rect,key,"12345"));}}
        else {attrs.push((xml.child(grad,"lin").unwrap(),"ang","7654321"));}
        for &(node,key,foreign) in &attrs {let original=xml.attr(node,key).unwrap().to_owned();xml.remove_attr(node,key);xml.set_attr(node,&format!("fixture:{key}"),foreign);xml.set_attr(node,key,&original);}
        s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());
        let ShapeKind::Auto(a)=&s.p().slides[0].shapes[index].kind else {panic!("auto")};assert_eq!(a.fill,fill,"foreign attributes must not override unqualified gradient state");
        let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].id;let frame=s.p().slides[0].shapes[index].frame;
        let saved=s.run(vec![Command::SetFrame {slide,shape,frame:frame.translated(127_000,254_000)}]);let package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();
        let grad=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="gradFill").last().unwrap();let gs=xml.elements(xml.child(grad,"gsLst").unwrap()).next().unwrap();assert_eq!(xml.attr(gs,"fixture:pos"),Some("12345"));assert_eq!(xml.attr(gs,"pos"),Some("0"));
        if is_path {let path=xml.child(grad,"path").unwrap();assert_eq!(xml.attr(path,"fixture:path"),Some("rect"));assert_eq!(xml.attr(path,"path"),Some("circle"));let rect=xml.child(path,"fillToRect").unwrap();for key in ["l","t","r","b"] {assert_eq!(xml.attr(rect,&format!("fixture:{key}")),Some("12345"));assert_eq!(xml.attr(rect,key),Some("50000"));}}
        else {let lin=xml.child(grad,"lin").unwrap();assert_eq!(xml.attr(lin,"fixture:ang"),Some("7654321"));assert_eq!(xml.attr(lin,"ang"),Some("0"));}
    }
}

#[test]
fn gradient_reader_ignores_foreign_geometry_children() {
    const NS: &str = "urn:ibis:test:gradient-reader";
    for (name,scenario) in [("gsLst","gradient-reader-gsLst"),("gs","gradient-reader-gs"),("lin","gradient-reader-lin"),("path","gradient-reader-path")] {
        let mut s=Scenario::open(scenario,"editing/pptx-plan.pptx");
        let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();
        let stops=vec![ibis_doc_model::GradientStop {position:0,color:Color::BLACK},ibis_doc_model::GradientStop {position:100_000,color:Color::WHITE}];
        let fill=Fill::Linear {angle:0,scaled:false,stops:stops.clone()};let auto=AutoShape::new(Geometry::Rect,fill.clone(),None);
        let data=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto))}]);
        let mut package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();
        xml.set_attr(root,"xmlns:fixture",NS);xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignore=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignore.trim());
        let grad=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="gradFill").last().unwrap();let list=xml.child(grad,"gsLst").unwrap();
        let parent=if name=="gs" {list} else {grad};let foreign=xml.insert_element(parent,0,&format!("fixture:{name}"));xml.set_attr(foreign,"fixture:keep",name);
        if name=="gsLst" || name=="gs" {let stop=if name=="gs" {foreign} else {xml.append_element(foreign,"a:gs")};xml.set_attr(stop,"pos","12345");let color=xml.append_element(stop,"a:srgbClr");xml.set_attr(color,"val","ABCDEF");}
        if name=="lin" {xml.set_attr(foreign,"ang","7654321");}
        if name=="path" {xml.set_attr(foreign,"path","rect");let rect=xml.append_element(foreign,"a:fillToRect");xml.set_attr(rect,"l","12345");}
        let payload=xml.append_element(foreign,"fixture:payload");xml.append_text(payload,"foreign gradient child retained");
        s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());
        let ShapeKind::Auto(a)=&s.p().slides[0].shapes[index].kind else {panic!("auto")};assert_eq!(a.fill,fill,"foreign {name} must not become known gradient state");
        let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].id;
        let saved=s.run(vec![Command::SetFill {slide,shape,fill:Fill::Linear {angle:7_407_360,scaled:false,stops}}]);let package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();
        let foreign=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.namespace(n)==Some(NS)&&xml.local_name(n)==name).expect("foreign node retained after geometry editing/UndoRedo/save");assert_eq!(xml.attr(foreign,"fixture:keep"),Some(name));assert_eq!(xml.text(xml.child(foreign,"payload").unwrap()),"foreign gradient child retained");
        match name {"gsLst"|"gs"=>{let gs=if name=="gs" {foreign} else {xml.child(foreign,"gs").unwrap()};assert_eq!(xml.attr(gs,"pos"),Some("12345"));assert_eq!(xml.attr(xml.child(gs,"srgbClr").unwrap(),"val"),Some("ABCDEF"));},"lin"=>assert_eq!(xml.attr(foreign,"ang"),Some("7654321")),"path"=>{assert_eq!(xml.attr(foreign,"path"),Some("rect"));assert_eq!(xml.attr(xml.child(foreign,"fillToRect").unwrap(),"l"),Some("12345"));},_=>unreachable!()}
    }
}

#[test]
fn gradient_geometry_keeps_unknown_properties() {
    const NS:&str="urn:ibis:test:gradient-geometry";
    let mut s=Scenario::open("gradient-geometry-unknown","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();let stops=vec![ibis_doc_model::GradientStop {position:0,color:Color::BLACK},ibis_doc_model::GradientStop {position:100_000,color:Color::WHITE}];let auto=AutoShape::new(Geometry::Rect,Fill::Linear {angle:0,scaled:false,stops:stops.clone()},None);let saved=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto))}]);
    let mut package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture",NS);xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignore=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignore.trim());let grad=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="gradFill").last().unwrap();xml.set_attr(grad,"fixture:keep","gradient");xml.set_attr(grad,"rotWithShape","0");let lin=xml.child(grad,"lin").unwrap();xml.set_attr(lin,"scaled","1");xml.set_attr(lin,"fixture:keep","line");let list=xml.child(grad,"gsLst").unwrap();xml.set_attr(list,"fixture:keep","list");let nodes=xml.elements(list).collect::<Vec<_>>();for (i,n) in nodes.iter().copied().enumerate() {xml.set_attr(n,"fixture:index",&i.to_string());let color=xml.child(n,"srgbClr").unwrap();xml.set_attr(color,"fixture:keep","color");let payload=xml.append_element(color,"fixture:payload");xml.append_text(payload,"色の未知内容 😀");}xml.insert(list,0,nodes[1]);let between=xml.insert_element(list,1,"fixture:between");xml.append_text(between,"between stops");let payload=xml.append_element(grad,"fixture:payload");xml.append_text(payload,"gradient retained");
    s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].id;
    for (angle,left,right) in [(7_407_360,10_000,90_000),(2_700_000,0,100_000),(5_400_000,50_000,50_000)] {
        let mut stops=stops.clone();stops[0].position=left;stops[1].position=right;let saved=s.run(vec![Command::SetFill {slide,shape,fill:Fill::Linear {angle,scaled:true,stops}}]);let package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();let grad=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("gradient")).expect("gradient attributes retained");assert_eq!(xml.attr(grad,"rotWithShape"),Some("0"));assert_eq!(xml.text(xml.child(grad,"payload").unwrap()),"gradient retained");let lin=xml.child(grad,"lin").unwrap();assert_eq!(xml.attr(lin,"scaled"),Some("1"));assert_eq!(xml.attr(lin,"fixture:keep"),Some("line"));let list=xml.child(grad,"gsLst").unwrap();assert_eq!(xml.attr(list,"fixture:keep"),Some("list"));let between=xml.elements(list).nth(1).unwrap();assert_eq!(xml.namespace(between),Some(NS));assert_eq!(xml.text(between),"between stops");for (i,n) in xml.elements(list).filter(|&n|xml.namespace(n)==Some("http://schemas.openxmlformats.org/drawingml/2006/main")&&xml.local_name(n)=="gs").enumerate() {let expected_index=if left==right {i} else {[1,0][i]};assert_eq!(xml.attr(n,"fixture:index"),Some(expected_index.to_string().as_str()));assert_eq!(xml.attr(n,"pos").unwrap().parse::<u32>().unwrap(),[left,right][expected_index]);let color=xml.child(n,"srgbClr").unwrap();assert_eq!(xml.attr(color,"val"),Some(["000000","FFFFFF"][expected_index]));assert_eq!(xml.attr(color,"fixture:keep"),Some("color"));assert_eq!(xml.text(xml.child(color,"payload").unwrap()),"色の未知内容 😀");}
    }
}

#[test]
fn shape_fill_keeps_foreign_fill_siblings() {
    let mut s=Scenario::open("shape-fill-namespace","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();let auto=AutoShape::new(Geometry::Rect,Fill::Solid(Color::opaque(Rgb(0x112233))),None);let saved=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto))}]);
    let mut package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:shape-fill");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignorable=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignorable.trim());let pr=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="spPr").last().unwrap();xml.set_attr(pr,"fixture:keep","shape");for name in ["solidFill","noFill","gradFill"] {let n=xml.append_element(pr,&format!("fixture:{name}"));xml.set_attr(n,"fixture:keep",name);let color=xml.append_element(n,"a:srgbClr");xml.set_attr(color,"val","ABCDEF");let payload=xml.append_element(n,"fixture:payload");xml.append_text(payload,"図形塗り保持 😀");}
    s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].id;
    for fill in [Fill::Solid(Color {rgb:Rgb(0x445566),alpha:128}),Fill::None,Fill::Pattern {preset:"diagCross".into(),foreground:Color::BLACK,background:Color::WHITE}] {
        let saved=s.run(vec![Command::SetFill {slide,shape,fill}]);let package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();let pr=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("shape")).unwrap();for name in ["solidFill","noFill","gradFill"] {let n=xml.elements(pr).find(|&n|xml.namespace(n)==Some("urn:ibis:test:shape-fill")&&xml.local_name(n)==name).expect("foreign shape fill is retained");assert_eq!(xml.attr(n,"fixture:keep"),Some(name));assert_eq!(xml.attr(xml.child(n,"srgbClr").unwrap(),"val"),Some("ABCDEF"));assert_eq!(xml.text(xml.child(n,"payload").unwrap()),"図形塗り保持 😀");}
    }
}

#[test]
fn stroke_fill_keeps_foreign_fill_siblings() {
    let mut s=Scenario::open("stroke-fill-namespace","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();let auto=AutoShape::new(Geometry::Rect,Fill::None,Some(Stroke::solid(Color::opaque(Rgb(0x112233)),25_400)));let saved=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto))}]);
    let mut package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:stroke-fill");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignorable=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignorable.trim());let pr=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="ln").last().unwrap();xml.set_attr(pr,"fixture:keep","shape");for name in ["solidFill","noFill","gradFill"] {let n=xml.append_element(pr,&format!("fixture:{name}"));xml.set_attr(n,"fixture:keep",name);let color=xml.append_element(n,"a:srgbClr");xml.set_attr(color,"val","ABCDEF");let payload=xml.append_element(n,"fixture:payload");xml.append_text(payload,"図形塗り保持 😀");}
    s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].id;
    for stroke in [Some(Stroke::solid(Color {rgb:Rgb(0x445566),alpha:128},25_400)),None,Some(Stroke::solid(Color::opaque(Rgb(0x778899)),50_800))] {
        let saved=s.run(vec![Command::SetStroke {slide,shape,stroke}]);let package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();let pr=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("shape")).unwrap();for name in ["solidFill","noFill","gradFill"] {let n=xml.elements(pr).find(|&n|xml.namespace(n)==Some("urn:ibis:test:stroke-fill")&&xml.local_name(n)==name).expect("foreign stroke fill is retained");assert_eq!(xml.attr(n,"fixture:keep"),Some(name));assert_eq!(xml.attr(xml.child(n,"srgbClr").unwrap(),"val"),Some("ABCDEF"));assert_eq!(xml.text(xml.child(n,"payload").unwrap()),"図形塗り保持 😀");}
    }
}

#[test]
fn stroke_ignores_foreign_dash_and_ends() {
    let mut s=Scenario::open("stroke-properties-namespace","editing/pptx-plan.pptx");let slide=s.p().slides[0].id;let index=s.p().slides[0].shapes.len();let id=s.document.ids().new_shape_id();let auto=AutoShape::new(Geometry::Rect,Fill::None,Some(Stroke::solid(Color::opaque(Rgb(0x112233)),25_400)));let saved=s.run(vec![Command::InsertShape {slide,index,shape:Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto))}]);
    let mut package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml_mut("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:stroke-properties");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignorable=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignorable.trim());let pr=xml.descendants(root).into_iter().filter(|&n|xml.is_element(n)&&xml.local_name(n)=="ln").last().unwrap();xml.set_attr(pr,"fixture:keep","shape");for name in ["prstDash","headEnd","tailEnd"] {let n=xml.append_element(pr,&format!("fixture:{name}"));xml.set_attr(n,"fixture:keep",name);xml.set_attr(n,"val","dash");xml.set_attr(n,"type","triangle");xml.set_attr(n,"w","lg");xml.set_attr(n,"len","lg");let color=xml.append_element(n,"a:srgbClr");xml.set_attr(color,"val","ABCDEF");let payload=xml.append_element(n,"fixture:payload");xml.append_text(payload,"図形塗り保持 😀");}
    s.data=package.save(SaveOptions::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[0].id;let shape=s.p().slides[0].shapes[index].id;
    let ShapeKind::Auto(auto)=&s.p().slides[0].shapes[index].kind else {panic!("auto")};let original=auto.stroke.as_ref().unwrap();assert_eq!(original.dash,ibis_doc_model::LineDash::Solid);assert_eq!(original.head,LineEnd::None);assert_eq!(original.tail,LineEnd::None);
    for stroke in [Some(Stroke::solid(Color {rgb:Rgb(0x445566),alpha:128},25_400)),None,Some(Stroke::solid(Color::opaque(Rgb(0x778899)),50_800))] {
        let saved=s.run(vec![Command::SetStroke {slide,shape,stroke}]);let package=ibis_ooxml::Package::open(saved).unwrap();let xml=package.xml("ppt/slides/slide1.xml").unwrap();let root=xml.root_element().unwrap();let pr=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("shape")).unwrap();for name in ["prstDash","headEnd","tailEnd"] {let n=xml.elements(pr).find(|&n|xml.namespace(n)==Some("urn:ibis:test:stroke-properties")&&xml.local_name(n)==name).expect("foreign stroke fill is retained");assert_eq!(xml.attr(n,"fixture:keep"),Some(name));assert_eq!(xml.attr(xml.child(n,"srgbClr").unwrap(),"val"),Some("ABCDEF"));assert_eq!(xml.text(xml.child(n,"payload").unwrap()),"図形塗り保持 😀");}
    }
}

#[test]
fn background_fill_keeps_injected_properties() {
    fn check(data: &[u8]) {
        let package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
        let xml = package.xml("ppt/slides/slide1.xml").unwrap();
        let root = xml.root_element().unwrap();
        let bg = xml.path(root, &["cSld", "bg"]).unwrap();
        let pr = xml.child(bg, "bgPr").unwrap();
        assert_eq!(xml.attr(bg, "fixture:background"), Some("bg-flag"), "background attribute lost");
        assert_eq!(xml.attr(pr, "fixture:properties"), Some("pr-flag"), "background properties attribute lost");
        for name in ["solidFill","noFill","gradFill"] {let n=xml.elements(pr).find(|&n|xml.namespace(n)==Some("urn:ibis:test:background-preservation")&&xml.local_name(n)==name).expect("foreign background fill is retained");assert_eq!(xml.attr(xml.child(n,"srgbClr").unwrap(),"val"),Some("ABCDEF"));assert_eq!(xml.attr(n,"fixture:keep"),Some(name));}
        let payload = xml.child(pr, "payload").unwrap();
        assert_eq!(xml.namespace(payload), Some("urn:ibis:test:background-preservation"));
        assert_eq!(xml.text(payload), "背景の未知内容 & keep");
        let shadow = xml.path(pr, &["effectLst", "outerShdw"]).unwrap();
        for (key, value) in [("blurRad", "12700"), ("dist", "25400"), ("dir", "0"), ("fixture:shadow", "effect-flag")] {
            assert_eq!(xml.attr(shadow, key), Some(value));
        }
        assert_eq!(xml.attr(xml.child(shadow, "srgbClr").unwrap(), "val"), Some("334455"));
    }
    let mut s = Scenario::open("background-properties", "editing/pptx-plan.pptx");
    let mut package = ibis_ooxml::Package::open(s.data.clone()).unwrap();
    let xml = package.xml_mut("ppt/slides/slide1.xml").unwrap();
    let root = xml.root_element().unwrap();
    xml.set_attr(root, "xmlns:fixture", "urn:ibis:test:background-preservation");
    xml.set_attr(root, "xmlns:mc", "http://schemas.openxmlformats.org/markup-compatibility/2006");
    let ignorable = format!("{} fixture", xml.attr(root, "mc:Ignorable").unwrap_or(""));
    xml.set_attr(root, "mc:Ignorable", ignorable.trim());
    let csld = xml.child(root, "cSld").unwrap();
    if let Some(bg) = xml.child(csld, "bg") { xml.detach(bg); }
    let bg = xml.insert_element(csld, 0, "p:bg");
    xml.set_attr(bg, "fixture:background", "bg-flag");
    let pr = xml.append_element(bg, "p:bgPr");
    xml.set_attr(pr, "fixture:properties", "pr-flag");
    xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignorable=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignorable.trim());
    for name in ["solidFill","noFill","gradFill"] {let n=xml.append_element(pr,&format!("fixture:{name}"));xml.set_attr(n,"fixture:keep",name);let color=xml.append_element(n,"a:srgbClr");xml.set_attr(color,"val","ABCDEF");}

    let fill = xml.append_element(pr, "a:solidFill");
    let color = xml.append_element(fill, "a:srgbClr"); xml.set_attr(color, "val", "FFFFFF");
    let effects = xml.append_element(pr, "a:effectLst");
    let shadow = xml.append_element(effects, "a:outerShdw");
    for (key, value) in [("blurRad", "12700"), ("dist", "25400"), ("dir", "0"), ("fixture:shadow", "effect-flag")] { xml.set_attr(shadow, key, value); }
    let color = xml.append_element(shadow, "a:srgbClr"); xml.set_attr(color, "val", "334455");
    let payload = xml.append_element(pr, "fixture:payload"); xml.append_text(payload, "背景の未知内容 & keep");
    s.data = package.save(ibis_ooxml::SaveOptions::default()).unwrap();
    check(&s.data);
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let slide = s.p().slides[0].id;
    for fill in [Fill::Solid(Color::opaque(Rgb(0xaa3344))), Fill::Linear { angle: 5_400_000, scaled: false, stops: vec![ibis_doc_model::GradientStop { position: 0, color: Color::BLACK }, ibis_doc_model::GradientStop { position: 100_000, color: Color::WHITE }] }] {
        let saved = s.run(vec![Command::SetBackground { slide, fill }]);
        check(&saved);
        check(&s.opened.source.save(s.p(), SaveOptions { regenerate_xml: true, ..Default::default() }).unwrap());
        assert!(s.document.undo()); check(&s.opened.source.save(s.p(), SaveOptions::default()).unwrap());
        assert!(s.document.redo()); check(&s.opened.source.save(s.p(), SaveOptions::default()).unwrap());
    }
}


#[test]
fn script_font_split_keeps_injected_run_properties() {
    const NS: &str = "urn:ibis:test:run-font-preservation";
    fn check(data: &[u8], changed: bool) {
        let package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
        let xml = package.xml("ppt/slides/slide1.xml").unwrap();
        let root = xml.root_element().unwrap();
        let runs: Vec<_> = xml.descendants(root).into_iter().filter(|&n| xml.is_element(n) && xml.local_name(n) == "r" && xml.child(n, "rPr").is_some_and(|pr| xml.attr(pr, "fixture:flag") == Some("keep & 未知"))).collect();
        let text: String = runs.iter().map(|&r| xml.text(xml.child(r, "t").unwrap())).collect();
        assert_eq!(text, "A日B語C");
        assert_eq!(runs.len(), if changed { 5 } else { 1 });
        for r in runs {
            let pr = xml.child(r, "rPr").unwrap();
            assert_eq!(xml.namespace(xml.child(pr, "payload").unwrap()), Some(NS));
            assert_eq!(xml.text(xml.child(pr, "payload").unwrap()), "opaque <value> 😀");
            let cs = xml.child(pr, "cs").unwrap();
            assert_eq!(xml.attr(cs, "typeface"), Some("+mn-cs"));
            assert_eq!(xml.attr(cs, "fixture:cs"), Some("retained"));
            let fill = xml.child(pr, "solidFill").unwrap();
            assert_eq!(xml.attr(xml.child(fill, "schemeClr").unwrap(), "val"), Some("accent2"));
            let text = xml.text(xml.child(r, "t").unwrap());
            for (name, expected) in [("latin", "+mn-lt"), ("ea", "+mn-ea")] {
                let node = xml.child(pr, name).unwrap();
                assert_eq!(xml.attr(node, "fixture:font"), Some("retained"));
                if !changed || text == "A" || text == "C" { assert_eq!(xml.attr(node, "typeface"), Some(expected)); }
            }
        }
    }
    let mut s = Scenario::open("script-font-unknown", "editing/pptx-plan.pptx");
    let (slide, shape, _) = text_shapes(s.p(), 1).into_iter().next().unwrap();
    let mut body = s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().clone();
    let mut paragraph = body.paragraphs[0].clone();
    paragraph.runs = vec![Run { text: "A日B語C".into(), style: RunStyle::default() }];
    body.paragraphs = vec![paragraph];
    let saved = s.run(vec![Command::SetTextBody { slide, shape, body }]);
    let mut package = ibis_ooxml::Package::open(saved).unwrap();
    let xml = package.xml_mut("ppt/slides/slide1.xml").unwrap();
    let root = xml.root_element().unwrap();
    xml.set_attr(root, "xmlns:fixture", NS);
    xml.set_attr(root, "xmlns:mc", "http://schemas.openxmlformats.org/markup-compatibility/2006");
    let ignorable = format!("{} fixture", xml.attr(root, "mc:Ignorable").unwrap_or(""));
    xml.set_attr(root, "mc:Ignorable", ignorable.trim());
    let run = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "r" && xml.child(n, "t").is_some_and(|t| xml.text(t) == "A日B語C")).unwrap();
    let pr = xml.child(run, "rPr").unwrap();
    xml.set_attr(pr, "fixture:flag", "keep & 未知");
    for (name, face) in [("latin", "+mn-lt"), ("ea", "+mn-ea"), ("cs", "+mn-cs")] {
        let n = xml.child(pr, name).unwrap_or_else(|| xml.append_element(pr, &format!("a:{name}")));
        xml.set_attr(n, "typeface", face);
        xml.set_attr(n, if name == "cs" { "fixture:cs" } else { "fixture:font" }, "retained");
    }
    for n in xml.elements(pr).filter(|&n| matches!(xml.local_name(n), "solidFill" | "noFill" | "gradFill")).collect::<Vec<_>>() { xml.detach(n); }
    let fill = xml.insert_element(pr, 0, "a:solidFill");
    let color = xml.append_element(fill, "a:schemeClr"); xml.set_attr(color, "val", "accent2");
    let payload = xml.append_element(pr, "fixture:payload"); xml.append_text(payload, "opaque <value> 😀");
    s.data = package.save(ibis_ooxml::SaveOptions::default()).unwrap();
    check(&s.data, false);
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let (slide, shape, _) = text_shapes(s.p(), 1).into_iter().find(|(_, _, text)| text == "A日B語C").unwrap();
    let saved = s.run(vec![Command::SetCharacterFormat { slide, shape, start: 1, end: 4, format: ibis_doc_model::CharacterFormat { script_fonts: Some((Some("Carlito".into()), Some("Hiragino Sans".into()))), ..Default::default() } }]);
    check(&saved, true);
    // Scenario normalization ignores fonts; assert the selected script families explicitly.
    let reread = ibis_pptx::read(&saved).unwrap();
    let body = reread.slides[0].shapes.iter().filter_map(|s| s.text_body()).find(|b| b.text() == "A日B語C").unwrap();
    for run in &body.paragraphs[0].runs {
        let style = body.style.with_run(&run.style);
        if matches!(run.text.as_str(), "日" | "語") {
            assert_eq!(style.font.as_deref(), Some("Hiragino Sans"));
            assert_eq!(style.fallback_font.as_deref(), Some("Carlito"));
        } else if run.text == "B" {
            assert_eq!(style.font.as_deref(), Some("Carlito"));
            assert_eq!(style.fallback_font.as_deref(), Some("Hiragino Sans"));
        }
    }
    check(&s.opened.source.save(s.p(), SaveOptions { regenerate_xml: true, ..Default::default() }).unwrap(), true);
    assert!(s.document.undo()); check(&s.opened.source.save(s.p(), SaveOptions::default()).unwrap(), false);
    assert!(s.document.redo()); check(&s.opened.source.save(s.p(), SaveOptions::default()).unwrap(), true);
}


#[test]
fn script_font_split_preserves_each_source_run() {
    let mut s = Scenario::open("script-font-source-runs", "editing/pptx-plan.pptx");
    let (slide, shape, _) = text_shapes(s.p(), 1).into_iter().next().unwrap();
    let mut body = s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().clone();
    let mut p = body.paragraphs[0].clone();
    p.runs = ["A日B", "語C"].into_iter().map(|text| Run { text: text.into(), style: RunStyle::default() }).collect();
    body.paragraphs = vec![p];
    let saved = s.run(vec![Command::SetTextBody { slide, shape, body }]);
    let mut package = ibis_ooxml::Package::open(saved).unwrap();
    let xml = package.xml_mut("ppt/slides/slide1.xml").unwrap(); let root = xml.root_element().unwrap();
    xml.set_attr(root, "xmlns:fixture", "urn:ibis:test:source-run");
    xml.set_attr(root, "xmlns:mc", "http://schemas.openxmlformats.org/markup-compatibility/2006");
    let ignorable = format!("{} fixture", xml.attr(root, "mc:Ignorable").unwrap_or("")); xml.set_attr(root, "mc:Ignorable", ignorable.trim());
    for r in xml.descendants(root) {
        if xml.is_element(r) && xml.local_name(r) == "r" {
            let Some(t) = xml.child(r, "t") else { continue };
            let marker = match xml.text(t).as_str() { "A日B" => "first", "語C" => "second", _ => continue };
            let pr = xml.child(r, "rPr").unwrap(); xml.set_attr(pr, "fixture:origin", marker);
            let payload = xml.append_element(pr, "fixture:payload"); xml.append_text(payload, marker);
        }
    }
    s.data = package.save(ibis_ooxml::SaveOptions::default()).unwrap();
    s.opened = ibis_pptx::open(s.data.clone()).unwrap(); s.document = Document::new(s.opened.presentation.clone());
    let (slide, shape, _) = text_shapes(s.p(), 1).into_iter().find(|(_, _, text)| text == "A日B語C").unwrap();
    let saved = s.run(vec![Command::SetCharacterFormat { slide, shape, start: 1, end: 4, format: ibis_doc_model::CharacterFormat { script_fonts: Some((Some("Carlito".into()), Some("Hiragino Sans".into()))), ..Default::default() } }]);
    fn check(data: Vec<u8>) {
        let package = ibis_ooxml::Package::open(data).unwrap(); let xml = package.xml("ppt/slides/slide1.xml").unwrap(); let root = xml.root_element().unwrap();
        let mut seen = String::new();
        for r in xml.descendants(root) {
            if !xml.is_element(r) || xml.local_name(r) != "r" { continue }
            let Some(pr) = xml.child(r, "rPr") else { continue };
            let Some(origin) = xml.attr(pr, "fixture:origin") else { continue };
            let text = xml.text(xml.child(r, "t").unwrap()); seen.push_str(&text);
            assert_eq!(origin, if text.contains('語') || text == "C" { "second" } else { "first" }, "source run of {text}");
            assert_eq!(xml.text(xml.child(pr, "payload").unwrap()), origin);
        }
        assert_eq!(seen, "A日B語C");
    }
    check(saved);
    check(s.opened.source.save(s.p(), SaveOptions { regenerate_xml: true, ..Default::default() }).unwrap());
    assert!(s.document.undo()); check(s.opened.source.save(s.p(), SaveOptions::default()).unwrap());
    assert!(s.document.redo()); check(s.opened.source.save(s.p(), SaveOptions::default()).unwrap());
    // Also cover a model edit that explicitly coalesces equal-format runs.
    let saved = s.run(vec![Command::SetCharacterFormat { slide, shape, start: 0, end: 5, format: ibis_doc_model::CharacterFormat { script_fonts: Some((Some("Carlito".into()), Some("Carlito".into()))), ..Default::default() } }]);
    check(saved);
    let mut body = s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().clone();
    let style = body.paragraphs[0].runs[0].style.clone();
    body.paragraphs[0].runs = vec![Run { text: "A日B語C".into(), style }];
    let saved = s.run(vec![Command::SetTextBody { slide, shape, body }]);
    check(saved);

}


#[test]
fn paragraph_font_alignment_round_trip() {
    let mut s = Scenario::open("paragraph-font-align", "editing/pptx-plan.pptx");
    let (slide, shape, _) = text_shapes(s.p(), 1).into_iter().next().unwrap();
    for alignment in [ibis_doc_model::FontAlign::Auto, ibis_doc_model::FontAlign::Top, ibis_doc_model::FontAlign::Center, ibis_doc_model::FontAlign::Bottom, ibis_doc_model::FontAlign::Baseline] {
        let mut body = s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().clone();
        body.paragraphs[0].font_align = alignment;
        let saved = s.run(vec![Command::SetTextBody { slide, shape, body }]);
        let reread = ibis_pptx::read(&saved).unwrap();
        assert_eq!(reread.slides[0].shapes.iter().find_map(|s| s.text_body()).unwrap().paragraphs[0].font_align, alignment);
    }
    let slide = s.p().slides[0].id;
    for alignment in [ibis_doc_model::FontAlign::Auto, ibis_doc_model::FontAlign::Top, ibis_doc_model::FontAlign::Center, ibis_doc_model::FontAlign::Bottom, ibis_doc_model::FontAlign::Baseline] {
        let id = s.document.ids().new_shape_id();
        let mut body = TextBody::plain("large small", TextStyle::default()); body.paragraphs[0].font_align = alignment;
        let shape = Shape::new(id, EmuRect::new(100_000, 200_000, 3_000_000, 1_000_000), ShapeKind::TextBox(body));
        let saved = s.run(vec![Command::InsertShape { slide, index: s.p().slides[0].shapes.len(), shape }]);
        let reread = ibis_pptx::read(&saved).unwrap();
        assert_eq!(reread.slides[0].shapes.last().unwrap().text_body().unwrap().paragraphs[0].font_align, alignment);
    }
}


#[test]
fn table_copy_preserves_borders_and_transform() {
    let mut s = Scenario::open("table-copy-properties", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id; let id = s.document.ids().new_shape_id();
    let mut table = ibis_doc_model::Table::new_grid(2, 2, 3_000_000, 1_500_000);
    let mut stroke = Stroke::solid(Color::opaque(Rgb(0x123456)), 25_400).with_dash(ibis_doc_model::LineDash::DashDot);
    stroke.cap = ibis_doc_model::LineCap::Round; stroke.compound = ibis_doc_model::LineCompound::Double;
    table.rows[0].cells[0].borders = ibis_doc_model::CellBorders { left: Some(stroke.clone()), right: Some(stroke.clone()), top: Some(stroke.clone()), bottom: Some(stroke), ..Default::default() };
    let mut shape = Shape::new(id, EmuRect::new(100_000, 200_000, 3_000_000, 1_500_000), ShapeKind::Table(Box::new(table)));
    shape.rotation = 15 * 60_000; shape.flip_h = true;
    let index = s.p().slides[0].shapes.len();
    let saved = s.run(vec![Command::InsertShape { slide, index, shape }]);
    s.data = saved.clone(); s.opened = ibis_pptx::open(saved).unwrap(); s.document = Document::new(s.opened.presentation.clone());
    let slide = s.p().slides[0].id;
    let mut copied = s.p().slides[0].shapes[index].clone(); copied.id = s.document.ids().new_shape_id(); copied.frame = copied.frame.translated(182_880, 182_880);
    s.run(vec![Command::InsertShape { slide, index: index + 1, shape: copied }]);
    let mut merged = s.p().slides[0].shapes[index + 1].clone(); merged.id = s.document.ids().new_shape_id();
    let ShapeKind::Table(table) = &mut merged.kind else { panic!("table") };
    table.rows[0].cells[0].column_span = 2; table.rows[0].cells[1].merged = true;
    s.run(vec![Command::InsertShape { slide, index: index + 2, shape: merged }]);
}


#[test]
fn shared_font_alignment_reference_has_five_distinct_values() {
    let path = std::env::var_os("IBIS_PPTX_FONT_ALIGN_REFERENCE").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../ibisOffice-Test/test/coverage/out/gen/pptx-PTXT.pptx"));
    if !path.exists() { eprintln!("shared font alignment reference unavailable: {}", path.display()); return; }
    let data = std::fs::read(&path).unwrap();
    let presentation = ibis_pptx::read(&data).unwrap();
    let mut values = Vec::new();
    for body in presentation.slides.iter().flat_map(|s| &s.shapes).filter_map(|s| s.text_body()) {
        for paragraph in &body.paragraphs { if !values.contains(&paragraph.font_align) { values.push(paragraph.font_align); } }
    }
    assert_eq!(values.len(), 5, "shared PTXT fontAlgn auto/t/ctr/base/b remain distinct: {values:?}");
}


#[test]
fn auto_paragraph_copy_keeps_each_injected_ppr() {
    const NS: &str = "urn:ibis:test:auto-paragraph-preservation";
    fn check(data: Vec<u8>, copied: bool) {
        let package = ibis_ooxml::Package::open(data).unwrap(); let xml = package.xml("ppt/slides/slide1.xml").unwrap(); let root = xml.root_element().unwrap();
        for origin in ["source", "target"] {
            let pr = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "pPr" && xml.attr(n, "fixture:origin") == Some(origin)).unwrap_or_else(|| panic!("lost {origin} pPr"));
            assert_eq!(xml.attr(pr, "fontAlgn"), Some(if origin == "source" || copied { "auto" } else { "base" }));
            let payload = xml.child(pr, "payload").unwrap(); assert_eq!(xml.namespace(payload), Some(NS));
            assert_eq!(xml.attr(payload, "fixture:version"), Some("19")); assert_eq!(xml.text(payload), format!("{origin} 未知 <keep> & 😀"));
        }
    }
    let mut s = Scenario::open("auto-paragraph-unknown", "editing/pptx-plan.pptx");
    let (slide, shape, _) = text_shapes(s.p(), 1).into_iter().next().unwrap();
    let mut body = s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().clone();
    let mut source = body.paragraphs[0].clone(); source.runs = vec![Run { text: "source".into(), style: source.end_style.clone() }];
    let mut target = source.clone(); target.runs[0].text = "target".into(); body.paragraphs = vec![source, target];
    let saved = s.run(vec![Command::SetTextBody { slide, shape, body }]);
    let mut package = ibis_ooxml::Package::open(saved).unwrap(); let xml = package.xml_mut("ppt/slides/slide1.xml").unwrap(); let root = xml.root_element().unwrap();
    xml.set_attr(root, "xmlns:fixture", NS); xml.set_attr(root, "xmlns:mc", "http://schemas.openxmlformats.org/markup-compatibility/2006");
    let ignorable = format!("{} fixture", xml.attr(root, "mc:Ignorable").unwrap_or("")); xml.set_attr(root, "mc:Ignorable", ignorable.trim());
    for p in xml.descendants(root) {
        if !xml.is_element(p) || xml.local_name(p) != "p" { continue; }
        let Some(r) = xml.child(p, "r") else { continue }; let Some(t) = xml.child(r, "t") else { continue }; let text = xml.text(t);
        if text != "source" && text != "target" { continue; }
        let pr = xml.child(p, "pPr").unwrap_or_else(|| xml.insert_element(p, 0, "a:pPr")); xml.set_attr(pr, "fontAlgn", if text == "source" { "auto" } else { "base" }); xml.set_attr(pr, "fixture:origin", &text);
        let payload = xml.append_element(pr, "fixture:payload"); xml.set_attr(payload, "fixture:version", "19"); xml.append_text(payload, &format!("{text} 未知 <keep> & 😀"));
    }
    s.data = package.save(ibis_ooxml::SaveOptions::default()).unwrap(); check(s.data.clone(), false);
    s.opened = ibis_pptx::open(s.data.clone()).unwrap(); s.document = Document::new(s.opened.presentation.clone());
    let (slide, shape, _) = text_shapes(s.p(), 1).into_iter().find(|(_,_,text)| text == "source\ntarget").unwrap();
    assert_eq!(s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().paragraphs[0].font_align, ibis_doc_model::FontAlign::Auto);
    let saved = s.run(vec![Command::SetParagraphFormat { slide, shape, start: 0, end: 6, format: ibis_doc_model::ParagraphFormat { space_after: Some(275), ..Default::default() } }]); check(saved, false);
    let mut body = s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().clone();
    body.paragraphs[1].font_align = body.paragraphs[0].font_align; body.paragraphs[1].space_after = body.paragraphs[0].space_after;
    let saved = s.run(vec![Command::SetTextBody { slide, shape, body }]); check(saved, true);
    check(s.opened.source.save(s.p(), SaveOptions { regenerate_xml: true, ..Default::default() }).unwrap(), true);
    assert!(s.document.undo()); check(s.opened.source.save(s.p(), SaveOptions::default()).unwrap(), false);
    assert!(s.document.redo()); check(s.opened.source.save(s.p(), SaveOptions::default()).unwrap(), true);
}


#[test]
fn original_nested_group_hierarchy_read_and_safe_edit_gate() {
    use ibis_doc_model::GroupMember;
    const P:&str="http://schemas.openxmlformats.org/presentationml/2006/main";
    let mut s=Scenario::open("group-hierarchy-original","editing/pptx-plan.pptx");let mut package=ibis_ooxml::Package::open(s.data.clone()).unwrap();let xml=package.xml_mut("ppt/slides/slide3.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:group-hierarchy");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignore=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignore.trim());let tree=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.namespace(n)==Some(P)&&xml.local_name(n)=="spTree").unwrap();let shapes=xml.elements(tree).filter(|&n|xml.namespace(n)==Some(P)&&xml.local_name(n)=="sp").collect::<Vec<_>>();assert!(shapes.len()>=4);
    fn group(xml:&mut ibis_ooxml::XmlDocument,parent:ibis_ooxml::NodeId,id:&str,name:&str)->ibis_ooxml::NodeId {let node=xml.append_element(parent,"p:grpSp");xml.set_attr(node,"fixture:keep",name);let nv=xml.append_element(node,"p:nvGrpSpPr");let pr=xml.append_element(nv,"p:cNvPr");xml.set_attr(pr,"id",id);xml.set_attr(pr,"name",name);xml.append_element(nv,"p:cNvGrpSpPr");xml.append_element(nv,"p:nvPr");let pr=xml.append_element(node,"p:grpSpPr");let x=xml.append_element(pr,"a:xfrm");for tag in ["off","ext","chOff","chExt"] {let n=xml.append_element(x,&format!("a:{tag}"));let size=tag=="ext"||tag=="chExt";xml.set_attr(n,if size {"cx"} else {"x"},if size {"1"} else {"0"});xml.set_attr(n,if size {"cy"} else {"y"},if size {"1"} else {"0"});}node}
    let outer=group(xml,tree,"10000","outer");let inner=group(xml,outer,"10001","inner");for &node in &shapes[1..3] {let index=xml.children(inner).len();xml.insert(inner,index,node);}let index=xml.children(outer).len();xml.insert(outer,index,shapes[3]);let payload=xml.append_element(outer,"fixture:payload");xml.append_text(payload,"opaque group kept");
    s.data=package.save(Default::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=&s.p().slides[2];assert_eq!(slide.groups.len(),1);assert_eq!(slide.groups[0].native_id,"10000");assert_eq!(slide.groups[0].name,"outer");let GroupMember::Group(inner)=&slide.groups[0].children[0] else {panic!("nested")};assert_eq!(inner.native_id,"10001");assert_eq!(inner.name,"inner");let end=slide.shapes.len();assert_eq!(inner.children,vec![GroupMember::Shape(slide.shapes[end-3].id),GroupMember::Shape(slide.shapes[end-2].id)]);assert_eq!(slide.groups[0].children[1],GroupMember::Shape(slide.shapes[end-1].id));assert!(slide.valid_groups(&slide.groups));assert_eq!(s.opened.source.save(s.p(),Default::default()).unwrap(),s.data);
    let id=slide.id;let shape=slide.shapes[end-3].id;let mut frame=slide.shapes[end-3].frame;frame.x+=127_000;let data=s.run(vec![Command::SetFrame {slide:id,shape,frame}]);let reread=ibis_pptx::read(&data).unwrap();assert_eq!(reread.slides[2].groups,s.p().slides[2].groups);let package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml("ppt/slides/slide3.xml").unwrap();let root=xml.root_element().unwrap();let outer=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("outer")).unwrap();assert_eq!(xml.text(xml.child(outer,"payload").unwrap()),"opaque group kept");
    let mut preview=s.p().clone();preview.apply(Command::SetShapeGroups {slide:id,groups:vec![]}).unwrap();assert!(s.opened.source.save(&preview,Default::default()).is_err(),"opaque group ancestors cannot be silently discarded");
    let mut renamed=s.p().slides[2].groups.clone();renamed[0].name="opaque group renamed".into();let renamed_data=s.run(vec![Command::SetShapeGroups {slide:id,groups:renamed}]);let package=ibis_ooxml::Package::open(renamed_data).unwrap();let xml=package.xml("ppt/slides/slide3.xml").unwrap();let root=xml.root_element().unwrap();let group=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("outer")).unwrap();assert_eq!(xml.text(xml.child(group,"payload").unwrap()),"opaque group kept");assert!(s.document.undo());
    let removal=s.run(vec![Command::RemoveShape {slide:id,shape}]);let reread=ibis_pptx::read(&removal).unwrap();assert!(reread.slides[2].valid_groups(&reread.slides[2].groups));let GroupMember::Group(inner)=&s.p().slides[2].groups[0].children[0] else {panic!("remaining subgroup")};assert_eq!(inner.children.len(),1);let package=ibis_ooxml::Package::open(removal.clone()).unwrap();let xml=package.xml("ppt/slides/slide3.xml").unwrap();let root=xml.root_element().unwrap();let outer=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("outer")).unwrap();assert_eq!(xml.text(xml.child(outer,"payload").unwrap()),"opaque group kept");std::fs::write(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/pptx-edit/group-leaf-removed.pptx"),removal).unwrap();assert!(s.document.undo());
    let leaves=s.p().slides[2].shapes[s.p().slides[2].shapes.len()-3..].iter().map(|s|s.id).collect();let data=s.run(vec![Command::RemoveShapes {slide:id,shapes:leaves}]);assert!(s.p().slides[2].groups.is_empty());assert!(ibis_pptx::read(&data).unwrap().slides[2].groups.is_empty());std::fs::write(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/pptx-edit/group-all-leaves-removed.pptx"),data).unwrap();assert!(s.document.undo());
    let copy_id=s.document.ids().new_slide_id();let copy=s.p().duplicate_slide(id,copy_id).unwrap();s.run(vec![Command::InsertSlide {index:3,slide:copy},Command::SetFrame {slide:copy_id,shape,frame:ibis_doc_model::EmuRect::new(frame.x+127_000,frame.y,frame.width,frame.height)}]);
}


#[test]
fn create_nested_groups_and_ungroup_neutral_source_round_trip() {
    use ibis_doc_model::{ShapeGroup,GroupMember};
    let mut s=Scenario::open("group-create-ungroup","editing/pptx-plan.pptx");let slide=s.p().slides[2].id;let ids=s.p().slides[2].shapes.iter().map(|shape|shape.id).collect::<Vec<_>>();assert!(ids.len()>=4);
    let groups=vec![ShapeGroup {has_unmodelled_children:false,native_id:"10000".into(),name:"group & <日本語>".into(),children:vec![GroupMember::Shape(ids[1]),GroupMember::Shape(ids[2])]}];let data=s.run(vec![Command::SetShapeGroups {slide,groups:groups.clone()}]);let read=ibis_pptx::read(&data).unwrap();assert_eq!(read.slides[2].groups[0].name,groups[0].name);let mut nested=vec![ShapeGroup {has_unmodelled_children:false,native_id:"10001".into(),name:"outer".into(),children:vec![GroupMember::Group(Box::new(groups[0].clone())),GroupMember::Shape(ids[3])]}];
    let data=s.run(vec![Command::SetShapeGroups {slide,groups:nested.clone()}]);let read=ibis_pptx::read(&data).unwrap();assert_eq!(read.slides[2].groups.len(),1);assert_eq!(read.slides[2].groups[0].native_id,"10001");
    let mut opened=Scenario::open("group-ungroup-source","editing/pptx-plan.pptx");opened.data=data;opened.opened=ibis_pptx::open(opened.data.clone()).unwrap();opened.document=Document::new(opened.opened.presentation.clone());let slide=opened.p().slides[2].id;let mut inner=match &opened.p().slides[2].groups[0].children[0] {GroupMember::Group(g)=>(**g).clone(),_=>panic!("inner")};inner.name="inner renamed".into();let renamed=vec![ShapeGroup {children:vec![GroupMember::Group(Box::new(inner.clone())),opened.p().slides[2].groups[0].children[1].clone()],..opened.p().slides[2].groups[0].clone()}];opened.run(vec![Command::SetShapeGroups {slide,groups:renamed}]);opened.run(vec![Command::SetShapeGroups {slide,groups:vec![inner]}]);let cleared=opened.run(vec![Command::SetShapeGroups {slide,groups:vec![]}]);assert!(ibis_pptx::read(&cleared).unwrap().slides[2].groups.is_empty());
    // Unknown attributes on leaf shapes and their original namespace stay put.
    let mut package=ibis_ooxml::Package::open(s.data.clone()).unwrap();let xml=package.xml_mut("ppt/slides/slide3.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:fixture","urn:ibis:test:group-leaf");xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006");let ignore=format!("{} fixture",xml.attr(root,"mc:Ignorable").unwrap_or(""));xml.set_attr(root,"mc:Ignorable",ignore.trim());let tree=xml.child(xml.child(root,"cSld").unwrap(),"spTree").unwrap();let shape=xml.elements(tree).find(|&n|xml.namespace(n)==Some("http://schemas.openxmlformats.org/presentationml/2006/main")&&xml.local_name(n)=="cxnSp").unwrap();xml.set_attr(shape,"fixture:keep","leaf");let pr=xml.child(shape,"spPr").unwrap();let payload=xml.append_element(pr,"fixture:payload");xml.append_text(payload,"opaque leaf kept");let snapshot=xml.append_element(payload,"p:grpSp");let nv=xml.append_element(snapshot,"p:nvGrpSpPr");let c=xml.append_element(nv,"p:cNvPr");xml.set_attr(c,"id","40000");xml.set_attr(c,"name","opaque snapshot");xml.append_element(nv,"p:cNvGrpSpPr");xml.append_element(nv,"p:nvPr");xml.append_element(snapshot,"p:grpSpPr");
    s.data=package.save(Default::default()).unwrap();s.opened=ibis_pptx::open(s.data.clone()).unwrap();s.document=Document::new(s.opened.presentation.clone());let slide=s.p().slides[2].id;let ids=s.p().slides[2].shapes.iter().map(|shape|shape.id).collect::<Vec<_>>();let line=s.p().slides[2].shapes.iter().position(|shape|matches!(shape.kind,ShapeKind::Line(_))).unwrap();assert!(line>0);nested[0].children=vec![GroupMember::Shape(ids[line-1]),GroupMember::Shape(ids[line])];let data=s.run(vec![Command::SetShapeGroups {slide,groups:nested}]);let package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml("ppt/slides/slide3.xml").unwrap();let root=xml.root_element().unwrap();let leaf=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.attr(n,"fixture:keep")==Some("leaf")).unwrap();let payload=xml.child(xml.child(leaf,"spPr").unwrap(),"payload").unwrap();assert_eq!(xml.text(payload),"opaque leaf kept");let snapshot=xml.child(payload,"grpSp").expect("known-name group inside opaque leaf payload stays in payload");let nv=xml.child(snapshot,"nvGrpSpPr").unwrap();assert_eq!(xml.attr(xml.child(nv,"cNvPr").unwrap(),"id"),Some("40000"));
    let mut collision=s.p().clone();collision.slides[2].groups[0].native_id="00004".into();assert!(s.opened.source.save(&collision,Default::default()).is_err(),"native IDs collide numerically, including leading zeros");
}


#[test]
fn group_hidden_last_visible_leaf_keeps_original_group() {
    use ibis_doc_model::{ShapeGroup,GroupMember};let mut s=Scenario::open("group-hidden-child","editing/pptx-plan.pptx");let slide=s.p().slides[2].id;let ids=s.p().slides[2].shapes.iter().map(|s|s.id).collect::<Vec<_>>();let data=s.run(vec![Command::SetShapeGroups {slide,groups:vec![ShapeGroup {has_unmodelled_children:false,native_id:"11000".into(),name:"hidden child group".into(),children:vec![GroupMember::Shape(ids[1]),GroupMember::Shape(ids[2])]}]}]);let mut package=ibis_ooxml::Package::open(data).unwrap();let xml=package.xml_mut("ppt/slides/slide3.xml").unwrap();let root=xml.root_element().unwrap();let group=xml.descendants(root).into_iter().find(|&n|xml.is_element(n)&&xml.namespace(n)==Some("http://schemas.openxmlformats.org/presentationml/2006/main")&&xml.local_name(n)=="grpSp").unwrap();let first=xml.elements(group).find(|&n|xml.namespace(n)==Some("http://schemas.openxmlformats.org/presentationml/2006/main")&&xml.local_name(n)=="sp").unwrap();let nv=xml.child(first,"nvSpPr").unwrap();let c=xml.child(nv,"cNvPr").unwrap();xml.set_attr(c,"hidden","1");
    let data=package.save(Default::default()).unwrap();let opened=ibis_pptx::open(data.clone()).unwrap();let slide=opened.presentation.slides[2].id;let GroupMember::Shape(visible)=opened.presentation.slides[2].groups[0].children[0] else {panic!("visible member")};let mut doc=Document::new(opened.presentation.clone());doc.execute(vec![Command::RemoveShape {slide,shape:visible}]).unwrap();assert_eq!(doc.presentation().slides[2].groups.len(),1,"native group still owns the hidden shape");assert!(doc.presentation().slides[2].groups[0].children.is_empty());let saved=opened.source.save(doc.presentation(),Default::default()).unwrap();let reread=ibis_pptx::read(&saved).unwrap();assert_eq!(reread.slides[2].groups.len(),1);assert!(reread.slides[2].groups[0].children.is_empty());assert!(doc.undo());assert_eq!(doc.presentation(),&opened.presentation);assert_eq!(opened.source.save(doc.presentation(),Default::default()).unwrap(),data);assert!(doc.redo());assert_eq!(doc.presentation().slides[2].groups.len(),1);std::fs::write(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/pptx-edit/group-hidden-visible-removed.pptx"),saved).unwrap();
    let mut preview=opened.presentation.clone();preview.apply(Command::SetShapeGroups {slide,groups:vec![]}).unwrap();assert!(opened.source.save(&preview,Default::default()).is_err(),"ungroup must not discard an unmodelled hidden child");
}

#[test]
fn shape_and_background_picture_fill_round_trip() {
    let mut s = Scenario::open("picture-fill", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let auto_id = s.p().slides[0].shapes.iter().find(|s| matches!(s.kind, ShapeKind::Auto(_))).unwrap().id;

    // A valid 1x1 PNG
    let png_bytes: Vec<u8> = vec![
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
        0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
        0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41,
        0x54, 0x08, 0xd7, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
        0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xdd, 0x8d,
        0xb0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
        0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    let media = s.document.ids().add_media(ibis_doc_model::MediaFormat::Png, png_bytes);

    let fill = Fill::Image(media);
    let data = s.run(vec![
        Command::SetFill { slide, shape: auto_id, fill: fill.clone() },
        Command::SetBackground { slide, fill: fill.clone() },
    ]);

    // Check with read
    let reopened = ibis_pptx::read(&data).unwrap();
    let re_slide = &reopened.slides[0];
    assert!(matches!(re_slide.background, Fill::Image(_)));
    let auto_index = s.p().slides[0].shapes.iter().position(|s| s.id == auto_id).unwrap();
    let re_shape = &re_slide.shapes[auto_index];
    let ShapeKind::Auto(re_auto) = &re_shape.kind else { panic!("not auto") };
    assert!(matches!(re_auto.fill, Fill::Image(_)));

    // Verify XML has blipFill
    let package = ibis_ooxml::Package::open(data).unwrap();
    let xml = package.xml("ppt/slides/slide1.xml").unwrap();
    let root = xml.root_element().unwrap();
    let blip_fills: Vec<_> = xml.descendants(root).into_iter().filter(|&n| xml.is_element(n) && xml.local_name(n) == "blipFill").collect();
    assert!(blip_fills.len() >= 2, "Expected at least 2 blipFill elements (background and shape)");

    // Undo restores previous fill and background
    assert!(s.document.undo());
    assert_ne!(s.p().slides[0].background, fill);
}

#[test]
fn table_style_id_round_trip() {
    let mut s = Scenario::open("table-style-id", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let id = s.document.ids().new_shape_id();
    let index = s.p().slides[0].shapes.len();

    // 1. スタイルID付きのテーブルを挿入して保存
    let mut table = ibis_doc_model::Table::new_grid(2, 2, 3_000_000, 1_500_000);
    let style1 = "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}";
    table.style_id = Some(style1.to_string());
    let shape = Shape::new(id, EmuRect::new(500_000, 500_000, 3_000_000, 1_500_000), ShapeKind::Table(Box::new(table)));
    let saved = s.run(vec![Command::InsertShape { slide, index, shape }]);

    // 2. 再オープンして style_id を確認
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = package.xml("ppt/slides/slide1.xml").unwrap();
    let root = xml.root_element().unwrap();
    let style_elem = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "tableStyleId").expect("tableStyleId present");
    assert_eq!(xml.text(style_elem), style1);

    s.data = saved;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let slide = s.p().slides[0].id;
    let shape_id = s.p().slides[0].shapes[index].id;
    let ShapeKind::Table(re_table) = &s.p().slides[0].shapes[index].kind else { panic!("table") };
    assert_eq!(re_table.style_id.as_deref(), Some(style1));

    // 3. 差分編集で style_id を別のGUIDに変更
    let mut updated_table = (**re_table).clone();
    let style2 = "{69CFFB72-C8E0-4CED-8131-404D4D77372C}";
    updated_table.style_id = Some(style2.to_string());
    let saved2 = s.run(vec![Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Table(Box::new(updated_table))) }]);

    let package2 = ibis_ooxml::Package::open(saved2.clone()).unwrap();
    let xml2 = package2.xml("ppt/slides/slide1.xml").unwrap();
    let root2 = xml2.root_element().unwrap();
    let style_elem2 = xml2.descendants(root2).into_iter().find(|&n| xml2.is_element(n) && xml2.local_name(n) == "tableStyleId").expect("tableStyleId updated");
    assert_eq!(xml2.text(style_elem2), style2);

    s.data = saved2;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let ShapeKind::Table(re_table2) = &s.p().slides[0].shapes[index].kind else { panic!("table") };
    assert_eq!(re_table2.style_id.as_deref(), Some(style2));

    // 4. 差分編集で style_id を None（スタイルなし）に変更
    let mut none_table = (**re_table2).clone();
    none_table.style_id = None;
    let slide2 = s.p().slides[0].id;
    let saved3 = s.run(vec![Command::SetKind { slide: slide2, shape: shape_id, kind: Box::new(ShapeKind::Table(Box::new(none_table))) }]);

    let package3 = ibis_ooxml::Package::open(saved3.clone()).unwrap();
    let xml3 = package3.xml("ppt/slides/slide1.xml").unwrap();
    let root3 = xml3.root_element().unwrap();
    let style_elem3 = xml3.descendants(root3).into_iter().find(|&n| xml3.is_element(n) && xml3.local_name(n) == "tableStyleId");
    assert!(style_elem3.is_none(), "tableStyleId detached when None");

    s.data = saved3;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    let ShapeKind::Table(re_table3) = &s.opened.presentation.slides[0].shapes[index].kind else { panic!("table") };
    assert_eq!(re_table3.style_id, None);
}

#[test]
fn transition_direction_round_trip() {
    let mut s = Scenario::open("transition-direction", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;

    // 1. push トランジション（方向なし）を設定して保存
    let mut trans = ibis_doc_model::Transition::new("push");
    let saved = s.run(vec![Command::SetTransition { slide, transition: Some(trans.clone()) }]);

    s.data = saved;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let slide = s.p().slides[0].id;
    let t = s.p().slides[0].transition.as_ref().expect("transition present");
    assert_eq!(t.effect, "push");
    assert_eq!(t.direction, None);

    // 2. 方向を "l"（左から）に設定して差分保存
    trans.direction = Some("l".to_string());
    let saved2 = s.run(vec![Command::SetTransition { slide, transition: Some(trans.clone()) }]);

    // XML を直接確認
    let package = ibis_ooxml::Package::open(saved2.clone()).unwrap();
    let xml = package.xml("ppt/slides/slide1.xml").unwrap();
    let root = xml.root_element().unwrap();
    let push_elem = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "push").expect("push element found");
    assert_eq!(xml.attr(push_elem, "dir"), Some("l"));

    // 再読込して direction が Some("l") であることを検証
    s.data = saved2;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let slide = s.p().slides[0].id;
    let t2 = s.p().slides[0].transition.as_ref().expect("transition present");
    assert_eq!(t2.effect, "push");
    assert_eq!(t2.direction.as_deref(), Some("l"));

    // 3. 方向を "u"（上から）に変更して差分保存
    trans.direction = Some("u".to_string());
    let saved3 = s.run(vec![Command::SetTransition { slide, transition: Some(trans.clone()) }]);

    // Undo / Redo 検証
    assert!(s.document.undo());
    assert_eq!(s.p().slides[0].transition.as_ref().and_then(|t| t.direction.as_deref()), Some("l"));
    assert!(s.document.redo());
    assert_eq!(s.p().slides[0].transition.as_ref().and_then(|t| t.direction.as_deref()), Some("u"));

    let package3 = ibis_ooxml::Package::open(saved3.clone()).unwrap();
    let xml3 = package3.xml("ppt/slides/slide1.xml").unwrap();
    let root3 = xml3.root_element().unwrap();
    let push_elem3 = xml3.descendants(root3).into_iter().find(|&n| xml3.is_element(n) && xml3.local_name(n) == "push").expect("push element found");
    assert_eq!(xml3.attr(push_elem3, "dir"), Some("u"));

    s.data = saved3;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let slide = s.p().slides[0].id;
    let t3 = s.opened.presentation.slides[0].transition.as_ref().expect("transition present");
    assert_eq!(t3.direction.as_deref(), Some("u"));

    // 4. split トランジション（horzIn）を設定して差分保存
    let mut split_trans = ibis_doc_model::Transition::new("split");
    split_trans.direction = Some("horzIn".to_string());
    let saved4 = s.run(vec![Command::SetTransition { slide, transition: Some(split_trans) }]);

    let package4 = ibis_ooxml::Package::open(saved4.clone()).unwrap();
    let xml4 = package4.xml("ppt/slides/slide1.xml").unwrap();
    let root4 = xml4.root_element().unwrap();
    let split_elem = xml4.descendants(root4).into_iter().find(|&n| xml4.is_element(n) && xml4.local_name(n) == "split").expect("split element found");
    assert_eq!(xml4.attr(split_elem, "orient"), Some("horz"));
    assert_eq!(xml4.attr(split_elem, "dir"), Some("in"));

    // 再読込して direction が Some("horzIn") であることを検証
    s.data = saved4;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    let t4 = s.opened.presentation.slides[0].transition.as_ref().expect("split transition present");
    assert_eq!(t4.effect, "split");
    assert_eq!(t4.direction.as_deref(), Some("horzIn"));
}

#[test]
fn show_properties_round_trip() {
    let mut s = Scenario::open("show-properties", "editing/pptx-plan.pptx");

    // 1. 初期状態を確認
    let initial = s.p().show_properties.clone();
    assert_eq!(initial.show_type, ibis_doc_model::ShowType::Presenter);
    assert!(!initial.loop_continuously);
    assert!(initial.show_animation);
    assert!(initial.use_timings);

    // 2. ショー設定を変更して保存
    let updated = ibis_doc_model::ShowProperties {
        show_type: ibis_doc_model::ShowType::Browse,
        loop_continuously: true,
        show_narration: false,
        show_animation: false,
        use_timings: false,
        slide_range: ibis_doc_model::ShowSlideRange::Range { start: 1, end: 3 },
        pen_color: Some(ibis_doc_model::drawing::Rgb(0x00B050)),
    };
    let saved = s.run(vec![Command::SetShowProperties { properties: updated.clone() }]);

    // XML を直接確認
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = package.xml("ppt/presProps.xml").unwrap();
    let root = xml.root_element().unwrap();
    let show_pr = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "showPr").expect("showPr found");
    assert_eq!(xml.attr(show_pr, "loop"), Some("1"));
    assert_eq!(xml.attr(show_pr, "showNarration"), Some("0"));
    assert_eq!(xml.attr(show_pr, "showAnimation"), Some("0"));
    assert_eq!(xml.attr(show_pr, "useTimings"), Some("0"));

    let has_browse = xml.elements(show_pr).any(|n| xml.local_name(n) == "browse");
    assert!(has_browse, "browse element present");

    let sld_rg = xml.elements(show_pr).find(|&n| xml.local_name(n) == "sldRg").expect("sldRg found");
    assert_eq!(xml.attr(sld_rg, "st"), Some("1"));
    assert_eq!(xml.attr(sld_rg, "end"), Some("3"));

    let pen_clr = xml.elements(show_pr).find(|&n| xml.local_name(n) == "penClr").expect("penClr found");
    let srgb = xml.elements(pen_clr).find(|&n| xml.local_name(n) == "srgbClr").expect("srgbClr found");
    assert_eq!(xml.attr(srgb, "val"), Some("00B050"));

    // 再読込して ShowProperties が完全一致することを検証
    s.data = saved;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    assert_eq!(s.p().show_properties, updated);

    // 3. キオスク端末モードに変更して Undo/Redo と再読込を検証
    let kiosk_props = ibis_doc_model::ShowProperties {
        show_type: ibis_doc_model::ShowType::Kiosk { restart: Some(300_000) },
        loop_continuously: true,
        show_narration: true,
        show_animation: true,
        use_timings: true,
        slide_range: ibis_doc_model::ShowSlideRange::All,
        pen_color: None,
    };
    let saved2 = s.run(vec![Command::SetShowProperties { properties: kiosk_props.clone() }]);

    // Undo / Redo 検証
    assert!(s.document.undo());
    assert_eq!(s.p().show_properties, updated);
    assert!(s.document.redo());
    assert_eq!(s.p().show_properties, kiosk_props);

    // キオスクの XML を直接確認
    let package2 = ibis_ooxml::Package::open(saved2.clone()).unwrap();
    let xml2 = package2.xml("ppt/presProps.xml").unwrap();
    let root2 = xml2.root_element().unwrap();
    let show_pr2 = xml2.descendants(root2).into_iter().find(|&n| xml2.is_element(n) && xml2.local_name(n) == "showPr").expect("showPr found");
    assert_eq!(xml2.attr(show_pr2, "loop"), Some("1"));
    assert!(xml2.attr(show_pr2, "showAnimation").is_none()); // default true -> 属性省略または1

    let kiosk = xml2.elements(show_pr2).find(|&n| xml2.local_name(n) == "kiosk").expect("kiosk found");
    assert_eq!(xml2.attr(kiosk, "restart"), Some("300000"));

    let has_all = xml2.elements(show_pr2).any(|n| xml2.local_name(n) == "sldAll");
    assert!(has_all, "sldAll element present");

    s.data = saved2;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    assert_eq!(s.opened.presentation.show_properties, kiosk_props);
}

#[test]
fn animations_delay_round_trip() {
    use ibis_doc_model::{Animation, AnimationClass as C, AnimationLook as L, AnimationStart as S};
    let mut s = Scenario::open("animations-delay", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].clone();
    let (title, subtitle) = (slide.shapes[0].id, slide.shapes[1].id);
    let a = vec![
        Animation { shape: title, class: C::Entrance, start: S::OnClick, look: L::Fade, delay: 1000, duration: 500, preset: (10, 0), repeat_count: ibis_doc_model::AnimationRepeat::None, auto_reverse: false, rewind: false, accel: 0, decel: 0, after_effect: ibis_doc_model::AnimationAfterEffect::Hold, trigger: ibis_doc_model::AnimationTrigger::None , paragraph: None, chars: None, text_unit: None, path: Vec::new(), chart_element: None, chart_build: None, dgm_element: None, dgm_build: None },
        Animation { shape: subtitle, class: C::Entrance, start: S::WithPrevious, look: L::Appear, delay: 500, duration: 1, preset: (1, 0), repeat_count: ibis_doc_model::AnimationRepeat::None, auto_reverse: false, rewind: false, accel: 0, decel: 0, after_effect: ibis_doc_model::AnimationAfterEffect::Hold, trigger: ibis_doc_model::AnimationTrigger::None , paragraph: None, chars: None, text_unit: None, path: Vec::new(), chart_element: None, chart_build: None, dgm_element: None, dgm_build: None },
    ];
    let saved = s.run(vec![Command::SetAnimations { slide: slide.id, animations: a.clone() }]);

    // Undo / Redo
    assert!(s.document.undo());
    assert_eq!(s.p().slides[0].animations, slide.animations);
    assert!(s.document.redo());
    assert_eq!(s.p().slides[0].animations, a);

    // 再読込して delay が保持されていることを確認
    s.data = saved;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    let re_anim = &s.opened.presentation.slides[0].animations;
    assert_eq!(re_anim.len(), 2);
    assert_eq!(re_anim[0].delay, 1000);
    assert_eq!(re_anim[1].delay, 500);
}

#[test]
fn animations_effect_options_round_trip() {
    use ibis_doc_model::{Animation, AnimationClass as C, AnimationLook as L, AnimationSide, AnimationStart as S};
    let mut s = Scenario::open("anim-options", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].clone();
    let (title, subtitle) = (slide.shapes[0].id, slide.shapes[1].id);

    // 1. スライドイン (上から Top = 1) および スピン (半回転 180° = 10_800_000) を設定
    let a1 = vec![
        Animation { shape: title, class: C::Entrance, start: S::OnClick, look: L::Fly(AnimationSide::Top), delay: 0, duration: 500, preset: (2, 1), repeat_count: ibis_doc_model::AnimationRepeat::None, auto_reverse: false, rewind: false, accel: 0, decel: 0, after_effect: ibis_doc_model::AnimationAfterEffect::Hold, trigger: ibis_doc_model::AnimationTrigger::None , paragraph: None, chars: None, text_unit: None, path: Vec::new(), chart_element: None, chart_build: None, dgm_element: None, dgm_build: None },
        Animation { shape: subtitle, class: C::Emphasis, start: S::WithPrevious, look: L::Spin(10_800_000), delay: 0, duration: 2000, preset: (8, 0), repeat_count: ibis_doc_model::AnimationRepeat::None, auto_reverse: false, rewind: false, accel: 0, decel: 0, after_effect: ibis_doc_model::AnimationAfterEffect::Hold, trigger: ibis_doc_model::AnimationTrigger::None , paragraph: None, chars: None, text_unit: None, path: Vec::new(), chart_element: None, chart_build: None, dgm_element: None, dgm_build: None },
    ];
    let saved1 = s.run(vec![Command::SetAnimations { slide: slide.id, animations: a1.clone() }]);

    // XMLの presetSubtype を確認
    let package1 = ibis_ooxml::Package::open(saved1.clone()).unwrap();
    let xml1 = package1.xml("ppt/slides/slide1.xml").unwrap();
    let root1 = xml1.root_element().unwrap();
    let fly_node = xml1.descendants(root1).into_iter().find(|&n| xml1.is_element(n) && xml1.attr(n, "presetID") == Some("2")).expect("fly effect");
    assert_eq!(xml1.attr(fly_node, "presetSubtype"), Some("1"));

    // 再読込して AnimationSide::Top / Spin(10_800_000) が復元されることを確認
    s.data = saved1;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let re1 = &s.p().slides[0].animations;
    assert_eq!(re1[0].look, L::Fly(AnimationSide::Top));
    assert_eq!(re1[1].look, L::Spin(10_800_000));

    // 2. 効果のオプションを変更: スライドインを「左から (Left = 8)」へ変更
    let a2 = vec![
        Animation { shape: title, class: C::Entrance, start: S::OnClick, look: L::Fly(AnimationSide::Left), delay: 0, duration: 500, preset: (2, 8), repeat_count: ibis_doc_model::AnimationRepeat::None, auto_reverse: false, rewind: false, accel: 0, decel: 0, after_effect: ibis_doc_model::AnimationAfterEffect::Hold, trigger: ibis_doc_model::AnimationTrigger::None , paragraph: None, chars: None, text_unit: None, path: Vec::new(), chart_element: None, chart_build: None, dgm_element: None, dgm_build: None },
        Animation { shape: subtitle, class: C::Emphasis, start: S::WithPrevious, look: L::Scale(200_000, 200_000), delay: 0, duration: 2000, preset: (6, 0), repeat_count: ibis_doc_model::AnimationRepeat::None, auto_reverse: false, rewind: false, accel: 0, decel: 0, after_effect: ibis_doc_model::AnimationAfterEffect::Hold, trigger: ibis_doc_model::AnimationTrigger::None , paragraph: None, chars: None, text_unit: None, path: Vec::new(), chart_element: None, chart_build: None, dgm_element: None, dgm_build: None },
    ];
    let saved2 = s.run(vec![Command::SetAnimations { slide: slide.id, animations: a2.clone() }]);

    let package2 = ibis_ooxml::Package::open(saved2.clone()).unwrap();
    let xml2 = package2.xml("ppt/slides/slide1.xml").unwrap();
    let root2 = xml2.root_element().unwrap();
    let fly_node2 = xml2.descendants(root2).into_iter().find(|&n| xml2.is_element(n) && xml2.attr(n, "presetID") == Some("2")).expect("fly effect 2");
    assert_eq!(xml2.attr(fly_node2, "presetSubtype"), Some("8"));

    s.data = saved2;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let re2 = &s.p().slides[0].animations;
    assert_eq!(re2[0].look, L::Fly(AnimationSide::Left));
    assert_eq!(re2[1].look, L::Scale(200_000, 200_000));
}

#[test]
fn animations_zoom_float_wipe_round_trip() {
    use ibis_doc_model::{Animation, AnimationClass as C, AnimationLook as L, AnimationSide, AnimationStart as S};
    let mut s = Scenario::open("anim-zoom-float-wipe", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].clone();
    let (title, subtitle) = (slide.shapes[0].id, slide.shapes[1].id);

    // 1. Entrance: Zoom, Float In (Top), Wipe (Bottom)
    let a1 = vec![
        Animation { shape: title, class: C::Entrance, start: S::OnClick, look: L::Zoom, delay: 0, duration: 500, preset: (23, 0), repeat_count: ibis_doc_model::AnimationRepeat::None, auto_reverse: false, rewind: false, accel: 0, decel: 0, after_effect: ibis_doc_model::AnimationAfterEffect::Hold, trigger: ibis_doc_model::AnimationTrigger::None, paragraph: None, chars: None, text_unit: None, path: Vec::new(), chart_element: None, chart_build: None, dgm_element: None, dgm_build: None },
        Animation { shape: subtitle, class: C::Entrance, start: S::WithPrevious, look: L::Float(AnimationSide::Top), delay: 0, duration: 1000, preset: (30, 1), repeat_count: ibis_doc_model::AnimationRepeat::None, auto_reverse: false, rewind: false, accel: 0, decel: 0, after_effect: ibis_doc_model::AnimationAfterEffect::Hold, trigger: ibis_doc_model::AnimationTrigger::None, paragraph: None, chars: None, text_unit: None, path: Vec::new(), chart_element: None, chart_build: None, dgm_element: None, dgm_build: None },
        Animation { shape: title, class: C::Entrance, start: S::AfterPrevious, look: L::Wipe(AnimationSide::Bottom), delay: 0, duration: 500, preset: (22, 4), repeat_count: ibis_doc_model::AnimationRepeat::None, auto_reverse: false, rewind: false, accel: 0, decel: 0, after_effect: ibis_doc_model::AnimationAfterEffect::Hold, trigger: ibis_doc_model::AnimationTrigger::None, paragraph: None, chars: None, text_unit: None, path: Vec::new(), chart_element: None, chart_build: None, dgm_element: None, dgm_build: None },
    ];
    let saved1 = s.run(vec![Command::SetAnimations { slide: slide.id, animations: a1 }]);

    let package1 = ibis_ooxml::Package::open(saved1.clone()).unwrap();
    let xml1 = package1.xml("ppt/slides/slide1.xml").unwrap();
    let root1 = xml1.root_element().unwrap();
    let zoom_node = xml1.descendants(root1).into_iter().find(|&n| xml1.is_element(n) && xml1.attr(n, "presetID") == Some("23")).expect("zoom effect");
    assert_eq!(xml1.attr(zoom_node, "presetSubtype"), Some("0"));
    let float_node = xml1.descendants(root1).into_iter().find(|&n| xml1.is_element(n) && xml1.attr(n, "presetID") == Some("30")).expect("float effect");
    assert_eq!(xml1.attr(float_node, "presetSubtype"), Some("1"));
    let wipe_node = xml1.descendants(root1).into_iter().find(|&n| xml1.is_element(n) && xml1.attr(n, "presetID") == Some("22")).expect("wipe effect");
    assert_eq!(xml1.attr(wipe_node, "presetSubtype"), Some("4"));

    s.data = saved1;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let re1 = &s.p().slides[0].animations;
    assert_eq!(re1[0].look, L::Zoom);
    assert_eq!(re1[1].look, L::Float(AnimationSide::Top));
    assert_eq!(re1[2].look, L::Wipe(AnimationSide::Bottom));

    // 2. Exit: Zoom, Float Out (Bottom = Down), Wipe (Top = Up)
    let a2 = vec![
        Animation { shape: title, class: C::Exit, start: S::OnClick, look: L::Zoom, delay: 0, duration: 500, preset: (23, 0), repeat_count: ibis_doc_model::AnimationRepeat::None, auto_reverse: false, rewind: false, accel: 0, decel: 0, after_effect: ibis_doc_model::AnimationAfterEffect::Hold, trigger: ibis_doc_model::AnimationTrigger::None, paragraph: None, chars: None, text_unit: None, path: Vec::new(), chart_element: None, chart_build: None, dgm_element: None, dgm_build: None },
        Animation { shape: subtitle, class: C::Exit, start: S::WithPrevious, look: L::Float(AnimationSide::Bottom), delay: 0, duration: 1000, preset: (30, 2), repeat_count: ibis_doc_model::AnimationRepeat::None, auto_reverse: false, rewind: false, accel: 0, decel: 0, after_effect: ibis_doc_model::AnimationAfterEffect::Hold, trigger: ibis_doc_model::AnimationTrigger::None, paragraph: None, chars: None, text_unit: None, path: Vec::new(), chart_element: None, chart_build: None, dgm_element: None, dgm_build: None },
        Animation { shape: title, class: C::Exit, start: S::AfterPrevious, look: L::Wipe(AnimationSide::Top), delay: 0, duration: 500, preset: (22, 1), repeat_count: ibis_doc_model::AnimationRepeat::None, auto_reverse: false, rewind: false, accel: 0, decel: 0, after_effect: ibis_doc_model::AnimationAfterEffect::Hold, trigger: ibis_doc_model::AnimationTrigger::None, paragraph: None, chars: None, text_unit: None, path: Vec::new(), chart_element: None, chart_build: None, dgm_element: None, dgm_build: None },
    ];
    let saved2 = s.run(vec![Command::SetAnimations { slide: slide.id, animations: a2 }]);

    let package2 = ibis_ooxml::Package::open(saved2.clone()).unwrap();
    let xml2 = package2.xml("ppt/slides/slide1.xml").unwrap();
    let root2 = xml2.root_element().unwrap();
    let float_node2 = xml2.descendants(root2).into_iter().find(|&n| xml2.is_element(n) && xml2.attr(n, "presetID") == Some("30")).expect("float exit effect");
    assert_eq!(xml2.attr(float_node2, "presetSubtype"), Some("2"));
    let wipe_node2 = xml2.descendants(root2).into_iter().find(|&n| xml2.is_element(n) && xml2.attr(n, "presetID") == Some("22")).expect("wipe exit effect");
    assert_eq!(xml2.attr(wipe_node2, "presetSubtype"), Some("1"));

    s.data = saved2;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let re2 = &s.p().slides[0].animations;
    assert_eq!(re2[0].look, L::Zoom);
    assert_eq!(re2[1].look, L::Float(AnimationSide::Bottom));
    assert_eq!(re2[2].look, L::Wipe(AnimationSide::Top));
}

#[test]
fn custom_shows_round_trip() {
    use ibis_doc_model::{CustomShow, ShowProperties, ShowSlideRange, ShowType};
    let mut s = Scenario::open("custom-shows", "editing/pptx-plan.pptx");
    assert!(s.p().custom_shows.is_empty());

    let s0 = s.p().slides[0].id;
    let s1 = s.p().slides[1].id;
    let s2 = s.p().slides[2].id;

    // 1. 2つの目的別スライドショーを作成して保存
    let shows = vec![
        CustomShow {
            id: 0,
            name: "ダイジェスト".to_string(),
            slides: vec![s0, s2],
        },
        CustomShow {
            id: 1,
            name: "第2章".to_string(),
            slides: vec![s1],
        },
    ];

    let saved = s.run(vec![Command::SetCustomShows { custom_shows: shows.clone() }]);

    // XML を直接確認
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = package.xml("ppt/presentation.xml").unwrap();
    let root = xml.root_element().unwrap();
    let list = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "custShowLst").expect("custShowLst found");

    let cust_shows = xml.elements(list).filter(|&n| xml.local_name(n) == "custShow").collect::<Vec<_>>();
    assert_eq!(cust_shows.len(), 2);
    assert_eq!(xml.attr(cust_shows[0], "name"), Some("ダイジェスト"));
    assert_eq!(xml.attr(cust_shows[0], "id"), Some("0"));
    assert_eq!(xml.attr(cust_shows[1], "name"), Some("第2章"));
    assert_eq!(xml.attr(cust_shows[1], "id"), Some("1"));

    let sld_lst0 = xml.elements(cust_shows[0]).find(|&n| xml.local_name(n) == "sldLst").expect("sldLst found");
    let slds0 = xml.elements(sld_lst0).filter(|&n| xml.local_name(n) == "sld").collect::<Vec<_>>();
    assert_eq!(slds0.len(), 2);

    // 再読込して検証
    s.data = saved;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    assert_eq!(s.p().custom_shows, shows);

    // 3. ショー設定で目的別スライドショー（id: 0）を指定して保存・再読込
    let show_props = ShowProperties {
        show_type: ShowType::Presenter,
        loop_continuously: true,
        show_narration: true,
        show_animation: true,
        use_timings: true,
        slide_range: ShowSlideRange::Custom(0),
        pen_color: None,
    };
    let saved2 = s.run(vec![Command::SetShowProperties { properties: show_props.clone() }]);
    s.data = saved2;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    assert_eq!(s.p().show_properties.slide_range, ShowSlideRange::Custom(0));
    assert_eq!(s.p().custom_shows, shows);

    // 4. カスタムショーを空にして保存 -> custShowLst が削除され、presProps.xml の dangling custShow が除去されることを検証
    let saved3 = s.run(vec![Command::SetCustomShows { custom_shows: Vec::new() }]);
    let package3 = ibis_ooxml::Package::open(saved3.clone()).unwrap();
    let xml3 = package3.xml("ppt/presentation.xml").unwrap();
    let root3 = xml3.root_element().unwrap();
    assert!(xml3.descendants(root3).into_iter().all(|n| !xml3.is_element(n) || xml3.local_name(n) != "custShowLst"));

    let pres_props_xml = package3.xml("ppt/presProps.xml").unwrap();
    let pres_props_root = pres_props_xml.root_element().unwrap();
    assert!(pres_props_xml.descendants(pres_props_root).into_iter().all(|n| !pres_props_xml.is_element(n) || pres_props_xml.local_name(n) != "custShow"), "presProps.xml does not have dangling custShow");

    s.data = saved3;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    assert!(s.opened.presentation.custom_shows.is_empty());
}

/// カスタム軌跡 (ANIM-005): a path of several lines is written as a line to each
/// point and read back with its points.
#[test]
fn animations_custom_path_round_trip() {
    use ibis_doc_model::{Animation, AnimationClass as C, AnimationLook as L, AnimationStart as S};
    let mut s = Scenario::open("anim-custom-path", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].clone();
    let title = slide.shapes[0].id;
    let path = vec![(10_000, 0), (10_000, 20_000), (-5_000, 20_000)];
    let animations = vec![Animation {
        shape: title,
        class: C::Path,
        start: S::OnClick,
        look: L::Motion(-5_000, 20_000),
        delay: 0,
        duration: 2000,
        preset: (0, 0),
        repeat_count: ibis_doc_model::AnimationRepeat::None,
        auto_reverse: false,
        rewind: false,
        accel: 0,
        decel: 0,
        after_effect: ibis_doc_model::AnimationAfterEffect::Hold,
        trigger: ibis_doc_model::AnimationTrigger::None,
        paragraph: None, chars: None, text_unit: None,
        path: path.clone(),
        chart_element: None, chart_build: None,
        dgm_element: None, dgm_build: None,
    }];
    let saved = s.run(vec![Command::SetAnimations { slide: slide.id, animations }]);
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = package.xml("ppt/slides/slide1.xml").unwrap();
    let root = xml.root_element().unwrap();
    let motion = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "animMotion").expect("animMotion");
    assert_eq!(xml.attr(motion, "path"), Some("M 0 0 L 0.1 0 L 0.1 0.2 L -0.05 0.2 E"));
    let effect = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.attr(n, "presetClass") == Some("path")).expect("path effect");
    assert_eq!(xml.attr(effect, "presetID"), Some("0"));
    let reopened = ibis_pptx::open(saved).unwrap();
    let back = &reopened.presentation.slides[0].animations;
    assert_eq!((back.len(), back[0].class, back[0].look), (1, C::Path, L::Motion(-5_000, 20_000)));
    assert_eq!(back[0].path, path);
}

/// ウォーターフォール・ヒストグラム・パレート図・じょうご (CHART-007, 008, 010): a new chartEx chart
/// is a part of its own with its workbook, in the slide as PowerPoint writes it, and reads back as the
/// kind it is.
#[test]
fn new_chart_ex_kinds_are_made_and_read_back() {
    use ibis_pptx::NewChartKind;
    for (kind, layout) in [(NewChartKind::Waterfall, "waterfall"), (NewChartKind::Histogram, "clusteredColumn"), (NewChartKind::Pareto, "paretoLine"), (NewChartKind::Funnel, "funnel"), (NewChartKind::BoxWhisker, "boxWhisker"), (NewChartKind::Treemap, "treemap"), (NewChartKind::Sunburst, "sunburst")] {
        let mut s = Scenario::open("chart-ex-new", "editing/pptx-plan.pptx");
        let slide = s.p().slides[0].id;
        let id = s.document.ids().new_shape_id();
        let frame = EmuRect::new(1_000_000, 1_500_000, 6_000_000, 3_000_000);
        let current = s.p().clone();
        let shape = s.opened.source.new_chart_kind(&current, slide, id, frame, kind).unwrap_or_else(|e| panic!("{kind:?}: {e}"));
        let ibis_doc_model::ShapeKind::Chart(chart) = &shape.kind else { panic!("{kind:?}: a chart") };
        assert!(chart.ex.is_some(), "{kind:?}: read as a chartEx kind");
        assert!(!chart.groups.is_empty() && chart.groups.iter().all(|g| g.series.iter().all(|x| !x.values.is_empty())), "{kind:?}: has its data");
        let saved = s.run(vec![Command::InsertShape { slide, index: s.p().slides[0].shapes.len(), shape }]);
        let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
        let part = package.names().find(|n| n.starts_with("ppt/charts/chartEx")).unwrap_or_else(|| panic!("{kind:?}: a chartEx part")).to_string();
        let xml = String::from_utf8(package.read(&part).unwrap()).unwrap();
        assert!(xml.contains(&format!("layoutId=\"{layout}\"")), "{kind:?}: {xml}");
        assert!(package.names().any(|n| n.starts_with("ppt/embeddings/") && n.ends_with(".xlsx")), "{kind:?}: its workbook");
        let types = String::from_utf8(package.read("[Content_Types].xml").unwrap()).unwrap();
        assert!(types.contains("application/vnd.ms-office.chartex+xml"), "{kind:?}");
        // Its style and colour style (PowerPoint and Excel do not open the chart without them).
        let rels = package.relationships(&part).unwrap();
        for (kind_url, content_type) in [("2011/relationships/chartStyle", "application/vnd.ms-office.chartstyle+xml"), ("2011/relationships/chartColorStyle", "application/vnd.ms-office.chartcolorstyle+xml")] {
            let rel = rels.iter().find(|r| r.kind.ends_with(kind_url)).unwrap_or_else(|| panic!("{kind:?}: no {kind_url}"));
            let target = ibis_ooxml::package::resolve("ppt/charts", &rel.target);
            assert!(package.contains(&target), "{kind:?}: {target}");
            assert_eq!(package.content_type(&target).unwrap().as_deref(), Some(content_type), "{kind:?}");
        }
        let slide_xml = String::from_utf8(package.read("ppt/slides/slide1.xml").unwrap()).unwrap();
        assert!(slide_xml.contains("mc:AlternateContent") && slide_xml.contains("Requires=\"cx1\""), "{kind:?}");
        // Read again: a chart of that kind.
        let again = ibis_pptx::open(saved).unwrap().presentation;
        let n = again.slides[0].shapes.iter().filter(|x| matches!(&x.kind, ibis_doc_model::ShapeKind::Chart(c) if c.ex.is_some())).count();
        assert_eq!(n, 1, "{kind:?}: one chartEx chart after reading");
    }
}

/// A chartEx chart's data edited in its workbook: the workbook's cells go into the part's caches.
#[test]
fn chart_ex_data_is_edited_in_its_workbook() {
    use ibis_doc_model::{CellEdit, CellInput, CellValue, Number, SheetCommand};
    use ibis_pptx::NewChartKind;
    let mut s = Scenario::open("chart-ex-data", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let id = s.document.ids().new_shape_id();
    let current = s.p().clone();
    let shape = s.opened.source.new_chart_kind(&current, slide, id, EmuRect::new(1_000_000, 1_500_000, 6_000_000, 3_000_000), NewChartKind::Waterfall).unwrap();
    let saved = s.run(vec![Command::InsertShape { slide, index: s.p().slides[0].shapes.len(), shape: shape.clone() }]);
    s.data = saved;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let current = s.p().clone();
    let read = current.slides[0].shapes.iter().find(|x| matches!(&x.kind, ibis_doc_model::ShapeKind::Chart(c) if c.ex.is_some())).unwrap().clone();
    // The workbook and the references of the chart.
    let data = s.opened.source.chart_data(&current, slide, read.id).expect("its data");
    assert_eq!((data.series.len(), data.series[0].values.as_deref(), data.series[0].categories.as_deref()), (1, Some("Sheet1!$B$2:$B$6"), Some("Sheet1!$A$2:$A$6")));
    // The second value (増加 1) changed in the workbook: 40 to 55; and the first category's text.
    let book = ibis_pptx::open(data.workbook.clone()).unwrap();
    let mut doc = Document::new(book.presentation.clone());
    let sheet_slide = doc.presentation().slides[0].id;
    doc.execute(vec![Command::Sheet(SheetCommand::SetCells {
        slide: sheet_slide,
        edits: vec![
            CellEdit { row: 2, column: 1, input: CellInput::Value(CellValue::Number(55.0)) },
            CellEdit { row: 1, column: 0, input: CellInput::Value(CellValue::Text("期首".into())) },
        ],
    })])
    .unwrap();
    let workbook = book.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    let edited = ibis_pptx::ChartData { workbook, series: data.series.clone() };
    let ibis_doc_model::ShapeKind::Chart(chart) = &read.kind else { panic!() };
    // As the program does: the chart read again from the file written with the data.
    let as_read = s.opened.source.chart_ex_with_data(&current, slide, read.id, edited).expect("the chart read again");
    let _ = chart;
    let saved = s.run(vec![Command::SetKind { slide, shape: read.id, kind: Box::new(as_read.kind.clone()) }]);
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let part = package.names().find(|n| n.starts_with("ppt/charts/chartEx")).unwrap().to_string();
    let xml = String::from_utf8(package.read(&part).unwrap()).unwrap();
    assert!(xml.contains(">55<") && xml.contains(">期首<") && !xml.contains(">40<"), "{xml}");
    assert!(xml.contains("layoutId=\"waterfall\""), "still a waterfall");
    // Read again: the new number is in the chart's values.
    let again = ibis_pptx::open(saved).unwrap().presentation;
    let values: Vec<f64> = again.slides[0].shapes.iter().filter_map(|x| match &x.kind { ibis_doc_model::ShapeKind::Chart(c) if c.ex.is_some() => Some(c), _ => None }).flat_map(|c| c.groups.iter().flat_map(|g| g.series.iter().flat_map(|se| se.values.iter().flatten().map(|Number(v)| *v).collect::<Vec<_>>()))).collect();
    assert!(values.contains(&55.0), "{values:?}");
}

/// 書き込みパスワード (REVIEW-014): `p:modifyVerifier` is written where the schema puts it,
/// read back, and taken away again.
#[test]
fn modify_password_round_trip() {
    use ibis_ooxml::modify_password::{hash, Algorithm};
    let mut s = Scenario::open("modify-password", "editing/pptx-plan.pptx");
    assert!(s.p().modify_verifier.is_none());
    let salt = [7u8, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22];
    let spin = 1000;
    let verifier = ibis_doc_model::ModifyVerifier { algorithm_sid: 14, spin_count: spin, salt: ibis_ooxml::base64::encode(&salt), hash: ibis_ooxml::base64::encode(&hash(Algorithm::Sha512, &salt, "ひみつ123", spin)) };
    let saved = s.run(vec![Command::SetModifyVerifier { verifier: Some(verifier.clone()) }]);
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = String::from_utf8(package.read("ppt/presentation.xml").unwrap()).unwrap();
    let at = xml.find("<p:modifyVerifier").expect("modifyVerifier");
    // After the default text style, and before the extension list when there is one.
    assert!(xml.find("<p:defaultTextStyle").is_none_or(|d| d < at), "{xml}");
    assert!(xml.find("<p:extLst").is_none_or(|e| at < e), "{xml}");
    assert!(xml.contains("cryptAlgorithmSid=\"14\"") && xml.contains("spinCount=\"1000\""));
    // Read again: the same, and the password is the one it checks.
    let back = ibis_pptx::open(saved.clone()).unwrap().presentation.modify_verifier.unwrap();
    assert_eq!(back, verifier);
    let salt_back = ibis_ooxml::base64::decode(&back.salt).unwrap();
    assert_eq!(ibis_ooxml::base64::encode(&hash(Algorithm::Sha512, &salt_back, "ひみつ123", back.spin_count)), back.hash);
    assert_ne!(ibis_ooxml::base64::encode(&hash(Algorithm::Sha512, &salt_back, "ひみつ", back.spin_count)), back.hash);
    // Taken away (the file is as the first was).
    s.data = saved;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let cleared = s.run(vec![Command::SetModifyVerifier { verifier: None }]);
    let package = ibis_ooxml::Package::open(cleared.clone()).unwrap();
    assert!(!String::from_utf8(package.read("ppt/presentation.xml").unwrap()).unwrap().contains("modifyVerifier"));
    assert!(ibis_pptx::open(cleared).unwrap().presentation.modify_verifier.is_none());
}

#[test]
fn animations_motion_path_round_trip() {
    use ibis_doc_model::{Animation, AnimationClass as C, AnimationLook as L, AnimationStart as S};
    let mut s = Scenario::open("anim-motion-path", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].clone();
    let title = slide.shapes[0].id;

    // 1. 直線モーションパス（下へ: x=0, y=25000, preset=(61, 4)）を設定
    let a1 = vec![Animation {
        shape: title,
        class: C::Path,
        start: S::OnClick,
        look: L::Motion(0, 25_000),
        delay: 0,
        duration: 2000,
        preset: (61, 4),
        repeat_count: ibis_doc_model::AnimationRepeat::None,
        auto_reverse: false,
        rewind: false,
        accel: 0,
        decel: 0,
        after_effect: ibis_doc_model::AnimationAfterEffect::Hold,
        trigger: ibis_doc_model::AnimationTrigger::None,
        paragraph: None, chars: None, text_unit: None, path: Vec::new(),
        chart_element: None, chart_build: None,
        dgm_element: None, dgm_build: None,
    }];
    let saved1 = s.run(vec![Command::SetAnimations { slide: slide.id, animations: a1.clone() }]);

    // XMLの presetID="61", presetSubtype="4", p:animMotion の path を確認
    let package1 = ibis_ooxml::Package::open(saved1.clone()).unwrap();
    let xml1 = package1.xml("ppt/slides/slide1.xml").unwrap();
    let root1 = xml1.root_element().unwrap();
    let motion_node = xml1.descendants(root1).into_iter().find(|&n| xml1.is_element(n) && xml1.attr(n, "presetID") == Some("61")).expect("motion path effect");
    assert_eq!(xml1.attr(motion_node, "presetSubtype"), Some("4"));
    let anim_motion = xml1.descendants(root1).into_iter().find(|&n| xml1.is_element(n) && xml1.local_name(n) == "animMotion").expect("animMotion element");
    assert_eq!(xml1.attr(anim_motion, "path"), Some("M 0 0 L 0 0.25 E"));

    // 再読込して AnimationClass::Path / Motion(0, 25_000) が復元されることを確認
    s.data = saved1;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let re1 = &s.p().slides[0].animations;
    assert_eq!(re1.len(), 1);
    assert_eq!(re1[0].class, C::Path);
    assert_eq!(re1[0].look, L::Motion(0, 25_000));
    assert_eq!(re1[0].preset, (61, 4));

    // 2. 効果オプションの変更: 「右へ」(x=25000, y=0, presetSubtype=2) に変更
    let a2 = vec![Animation {
        shape: title,
        class: C::Path,
        start: S::OnClick,
        look: L::Motion(25_000, 0),
        delay: 0,
        duration: 2000,
        preset: (61, 2),
        repeat_count: ibis_doc_model::AnimationRepeat::None,
        auto_reverse: false,
        rewind: false,
        accel: 0,
        decel: 0,
        after_effect: ibis_doc_model::AnimationAfterEffect::Hold,
        trigger: ibis_doc_model::AnimationTrigger::None,
        paragraph: None, chars: None, text_unit: None, path: Vec::new(),
        chart_element: None, chart_build: None,
        dgm_element: None, dgm_build: None,
    }];
    let saved2 = s.run(vec![Command::SetAnimations { slide: slide.id, animations: a2.clone() }]);

    let package2 = ibis_ooxml::Package::open(saved2.clone()).unwrap();
    let xml2 = package2.xml("ppt/slides/slide1.xml").unwrap();
    let root2 = xml2.root_element().unwrap();
    let motion_node2 = xml2.descendants(root2).into_iter().find(|&n| xml2.is_element(n) && xml2.attr(n, "presetID") == Some("61")).expect("motion path effect 2");
    assert_eq!(xml2.attr(motion_node2, "presetSubtype"), Some("2"));
    let anim_motion2 = xml2.descendants(root2).into_iter().find(|&n| xml2.is_element(n) && xml2.local_name(n) == "animMotion").expect("animMotion element 2");
    assert_eq!(xml2.attr(anim_motion2, "path"), Some("M 0 0 L 0.25 0 E"));

    s.data = saved2;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let re2 = &s.p().slides[0].animations;
    assert_eq!(re2[0].class, C::Path);
    assert_eq!(re2[0].look, L::Motion(25_000, 0));
    assert_eq!(re2[0].preset, (61, 2));
}

#[test]
fn shape_alt_text_round_trip() {
    use ibis_doc_model::AltText;
    let mut s = Scenario::open("alt-text", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].clone();
    let title_id = slide.shapes[0].id;

    // 初期状態では代替テキストなし
    assert!(s.p().slides[0].shape_alt_texts.get(&title_id).is_none());

    // 1. 代替テキスト（タイトル・説明文）を設定して保存
    let alt1 = AltText {
        title: "メイン見出し".to_string(),
        description: "プレゼンテーションの表紙タイトル図形".to_string(),
    };
    let saved1 = s.run(vec![Command::SetShapeAltText {
        slide: slide.id,
        shape: title_id,
        alt_text: alt1.clone(),
    }]);

    // XMLの cNvPr 属性（title, descr）を直接検証
    let package1 = ibis_ooxml::Package::open(saved1.clone()).unwrap();
    let xml1 = package1.xml("ppt/slides/slide1.xml").unwrap();
    let root1 = xml1.root_element().unwrap();
    let c_nv_pr1 = xml1.descendants(root1).into_iter().find(|&n| xml1.is_element(n) && xml1.local_name(n) == "cNvPr" && xml1.attr(n, "id") == Some("4")).expect("cNvPr for title shape");
    assert_eq!(xml1.attr(c_nv_pr1, "title"), Some("メイン見出し"));
    assert_eq!(xml1.attr(c_nv_pr1, "descr"), Some("プレゼンテーションの表紙タイトル図形"));

    // 再読込して AltText が復元されることを検証
    s.data = saved1;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let re1 = s.p().slides[0].shape_alt_texts.get(&title_id);
    assert_eq!(re1, Some(&alt1));

    // 2. 代替テキストを更新（説明文のみ変更）して保存
    let alt2 = AltText {
        title: "メイン見出し".to_string(),
        description: "更新された詳細説明".to_string(),
    };
    let saved2 = s.run(vec![Command::SetShapeAltText {
        slide: slide.id,
        shape: title_id,
        alt_text: alt2.clone(),
    }]);

    let package2 = ibis_ooxml::Package::open(saved2.clone()).unwrap();
    let xml2 = package2.xml("ppt/slides/slide1.xml").unwrap();
    let root2 = xml2.root_element().unwrap();
    let c_nv_pr2 = xml2.descendants(root2).into_iter().find(|&n| xml2.is_element(n) && xml2.local_name(n) == "cNvPr" && xml2.attr(n, "id") == Some("4")).expect("cNvPr 2");
    assert_eq!(xml2.attr(c_nv_pr2, "title"), Some("メイン見出し"));
    assert_eq!(xml2.attr(c_nv_pr2, "descr"), Some("更新された詳細説明"));

    s.data = saved2;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let re2 = s.p().slides[0].shape_alt_texts.get(&title_id);
    assert_eq!(re2, Some(&alt2));

    // 3. 代替テキストを空にして保存 -> 属性が削除されることを検証
    let empty_alt = AltText::default();
    let saved3 = s.run(vec![Command::SetShapeAltText {
        slide: slide.id,
        shape: title_id,
        alt_text: empty_alt,
    }]);

    let package3 = ibis_ooxml::Package::open(saved3.clone()).unwrap();
    let xml3 = package3.xml("ppt/slides/slide1.xml").unwrap();
    let root3 = xml3.root_element().unwrap();
    let c_nv_pr3 = xml3.descendants(root3).into_iter().find(|&n| xml3.is_element(n) && xml3.local_name(n) == "cNvPr" && xml3.attr(n, "id") == Some("4")).expect("cNvPr 3");
    assert_eq!(xml3.attr(c_nv_pr3, "title"), None);
    assert_eq!(xml3.attr(c_nv_pr3, "descr"), None);

    s.data = saved3;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    assert!(s.opened.presentation.slides[0].shape_alt_texts.get(&title_id).is_none());
}

#[test]
fn text_columns_round_trip() {
    let mut s = Scenario::open("text-columns", "editing/pptx-plan.pptx");
    let slide_id = s.p().slides[0].id;
    let (shape_id, mut body) = s.p().slides[0]
        .shapes
        .iter()
        .find_map(|sh| sh.text_body().map(|b| (sh.id, b.clone())))
        .expect("shape with text body");

    // 初期状態の確認
    assert_eq!(body.columns, 1);
    assert_eq!(body.column_spacing, 0);

    // 1. 2段組、段間隔 0.5cm (180,000 EMU) に設定して保存
    body.columns = 2;
    body.column_spacing = 180_000;
    let saved1 = s.run(vec![Command::SetTextBody {
        slide: slide_id,
        shape: shape_id,
        body: body.clone(),
    }]);

    // XML に numCol="2", spcCol="180000" が書き込まれていることを検証
    let package1 = ibis_ooxml::Package::open(saved1.clone()).unwrap();
    let xml1 = package1.xml("ppt/slides/slide1.xml").unwrap();
    let root1 = xml1.root_element().unwrap();
    let body_pr1 = xml1.descendants(root1).into_iter().find(|&n| xml1.is_element(n) && xml1.local_name(n) == "bodyPr").expect("bodyPr");
    assert_eq!(xml1.attr(body_pr1, "numCol"), Some("2"));
    assert_eq!(xml1.attr(body_pr1, "spcCol"), Some("180000"));

    // 再読込してモデルに復元されることを検証
    s.data = saved1;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let re1 = s.p().slides[0].shape(shape_id).unwrap().text_body().unwrap();
    assert_eq!(re1.columns, 2);
    assert_eq!(re1.column_spacing, 180_000);

    // 2. 3段組、段間隔 1cm (360,000 EMU) に変更して保存
    body.columns = 3;
    body.column_spacing = 360_000;
    let saved2 = s.run(vec![Command::SetTextBody {
        slide: slide_id,
        shape: shape_id,
        body: body.clone(),
    }]);

    let package2 = ibis_ooxml::Package::open(saved2.clone()).unwrap();
    let xml2 = package2.xml("ppt/slides/slide1.xml").unwrap();
    let root2 = xml2.root_element().unwrap();
    let body_pr2 = xml2.descendants(root2).into_iter().find(|&n| xml2.is_element(n) && xml2.local_name(n) == "bodyPr").expect("bodyPr");
    assert_eq!(xml2.attr(body_pr2, "numCol"), Some("3"));
    assert_eq!(xml2.attr(body_pr2, "spcCol"), Some("360000"));

    s.data = saved2;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let re2 = s.p().slides[0].shape(shape_id).unwrap().text_body().unwrap();
    assert_eq!(re2.columns, 3);
    assert_eq!(re2.column_spacing, 360_000);

    // 3. 1段（解除）に戻して保存 -> numCol, spcCol 属性が削除されることを検証
    body.columns = 1;
    body.column_spacing = 0;
    let saved3 = s.run(vec![Command::SetTextBody {
        slide: slide_id,
        shape: shape_id,
        body: body.clone(),
    }]);

    let package3 = ibis_ooxml::Package::open(saved3.clone()).unwrap();
    let xml3 = package3.xml("ppt/slides/slide1.xml").unwrap();
    let root3 = xml3.root_element().unwrap();
    let body_pr3 = xml3.descendants(root3).into_iter().find(|&n| xml3.is_element(n) && xml3.local_name(n) == "bodyPr").expect("bodyPr");
    assert_eq!(xml3.attr(body_pr3, "numCol"), None);
    assert_eq!(xml3.attr(body_pr3, "spcCol"), None);

    let re3_doc = s.p().slides[0].shape(shape_id).unwrap().text_body().unwrap();
    assert_eq!(re3_doc.columns, 1);
    assert_eq!(re3_doc.column_spacing, 0);

    // 4. Undo で 3段組に戻ることを検証
    assert!(s.document.undo());
    let re_undo = s.p().slides[0].shape(shape_id).unwrap().text_body().unwrap();
    assert_eq!(re_undo.columns, 3);
    assert_eq!(re_undo.column_spacing, 360_000);

    // 5. Redo で 1段に戻ることを検証
    assert!(s.document.redo());
    let re_redo = s.p().slides[0].shape(shape_id).unwrap().text_body().unwrap();
    assert_eq!(re_redo.columns, 1);
    assert_eq!(re_redo.column_spacing, 0);

    // 6. 保存したデータ saved3 を開き直してモデルに復元されることを検証
    s.data = saved3;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let re3 = s.p().slides[0].shape(shape_id).unwrap().text_body().unwrap();
    assert_eq!(re3.columns, 1);
    assert_eq!(re3.column_spacing, 0);

    // 7. 新規テキストボックスを挿入し、段組を設定して保存・再読込
    let new_id = s.document.ids().new_shape_id();
    let mut new_text_body = ibis_doc_model::TextBody::plain("カラム1\nカラム2", ibis_doc_model::TextStyle::default());
    new_text_body.columns = 2;
    new_text_body.column_spacing = 254_000;
    let new_shape = ibis_doc_model::Shape::new(
        new_id,
        ibis_doc_model::EmuRect::new(1_000_000, 1_000_000, 4_000_000, 2_000_000),
        ibis_doc_model::ShapeKind::Auto(ibis_doc_model::AutoShape::text_box(ibis_doc_model::Fill::None, new_text_body)),
    );
    let saved_new = s.run(vec![Command::InsertShape {
        slide: slide_id,
        index: s.p().slides[0].shapes.len(),
        shape: new_shape,
    }]);

    let package_new = ibis_ooxml::Package::open(saved_new.clone()).unwrap();
    let xml_new = package_new.xml("ppt/slides/slide1.xml").unwrap();
    let root_new = xml_new.root_element().unwrap();
    let body_prs: Vec<_> = xml_new.descendants(root_new).into_iter().filter(|&n| xml_new.is_element(n) && xml_new.local_name(n) == "bodyPr" && xml_new.attr(n, "numCol") == Some("2")).collect();
    assert!(!body_prs.is_empty(), "new shape has bodyPr with numCol=2");
    assert_eq!(xml_new.attr(body_prs[0], "spcCol"), Some("254000"));

    let reopened_new = ibis_pptx::open(saved_new).unwrap();
    let re_new_body = reopened_new.presentation.slides[0].shapes.last().unwrap().text_body().unwrap();
    assert_eq!(re_new_body.columns, 2);
    assert_eq!(re_new_body.column_spacing, 254_000);
}

#[test]
fn paragraph_indent_and_tabs_round_trip() {
    let mut s = Scenario::open("para-indent-tabs", "editing/pptx-plan.pptx");
    let slide_id = s.p().slides[0].id;
    let (shape_id, mut body) = s.p().slides[0]
        .shapes
        .iter()
        .find_map(|sh| sh.text_body().map(|b| (sh.id, b.clone())))
        .expect("shape with text body");

    // 1. 段落インデント（左余白・最初の行のぶら下げインデント）とタブ位置を設定
    body.paragraphs[0].margin_left = 457_200; // 0.5 inch (36 pt)
    body.paragraphs[0].indent = -228_600;     // -0.25 inch (-18 pt, hanging indent)
    body.paragraphs[0].tabs = vec![
        ibis_doc_model::TextTabStop { position: 1_440_000, align: ibis_doc_model::TextTabAlign::Left },
        ibis_doc_model::TextTabStop { position: 2_880_000, align: ibis_doc_model::TextTabAlign::Right },
        ibis_doc_model::TextTabStop { position: 4_320_000, align: ibis_doc_model::TextTabAlign::Decimal },
    ];
    let saved1 = s.run(vec![Command::SetTextBody {
        slide: slide_id,
        shape: shape_id,
        body: body.clone(),
    }]);

    // XML に marL, indent, tabLst が正しく出力されていることを検証
    let package1 = ibis_ooxml::Package::open(saved1.clone()).unwrap();
    let xml1 = package1.xml("ppt/slides/slide1.xml").unwrap();
    let root1 = xml1.root_element().unwrap();
    let ppr1 = xml1.descendants(root1).into_iter().find(|&n| xml1.is_element(n) && xml1.local_name(n) == "pPr").expect("pPr");
    assert_eq!(xml1.attr(ppr1, "marL"), Some("457200"));
    assert_eq!(xml1.attr(ppr1, "indent"), Some("-228600"));
    let tab_lst1 = xml1.descendants(ppr1).into_iter().find(|&n| xml1.is_element(n) && xml1.local_name(n) == "tabLst").expect("tabLst");
    let tabs1: Vec<_> = xml1.descendants(tab_lst1).into_iter().filter(|&n| xml1.is_element(n) && xml1.local_name(n) == "tab").collect();
    assert_eq!(tabs1.len(), 3);
    assert_eq!(xml1.attr(tabs1[0], "pos"), Some("1440000"));
    assert_eq!(xml1.attr(tabs1[0], "algn"), None); // default Left
    assert_eq!(xml1.attr(tabs1[1], "pos"), Some("2880000"));
    assert_eq!(xml1.attr(tabs1[1], "algn"), Some("r"));
    assert_eq!(xml1.attr(tabs1[2], "pos"), Some("4320000"));
    assert_eq!(xml1.attr(tabs1[2], "algn"), Some("dec"));

    // 再読込してモデルに復元されることを検証
    s.data = saved1;
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let re1 = s.p().slides[0].shape(shape_id).unwrap().text_body().unwrap();
    assert_eq!(re1.paragraphs[0].margin_left, 457_200);
    assert_eq!(re1.paragraphs[0].indent, -228_600);
    assert_eq!(re1.paragraphs[0].tabs.len(), 3);
    assert_eq!(re1.paragraphs[0].tabs[0].position, 1_440_000);
    assert_eq!(re1.paragraphs[0].tabs[0].align, ibis_doc_model::TextTabAlign::Left);
    assert_eq!(re1.paragraphs[0].tabs[1].position, 2_880_000);
    assert_eq!(re1.paragraphs[0].tabs[1].align, ibis_doc_model::TextTabAlign::Right);
    assert_eq!(re1.paragraphs[0].tabs[2].position, 4_320_000);
    assert_eq!(re1.paragraphs[0].tabs[2].align, ibis_doc_model::TextTabAlign::Decimal);

    // 2. タブ位置とインデントをクリアして保存・再読込
    body.paragraphs[0].tabs.clear();
    body.paragraphs[0].margin_left = 0;
    body.paragraphs[0].indent = 0;
    let saved2 = s.run(vec![Command::SetTextBody {
        slide: slide_id,
        shape: shape_id,
        body: body.clone(),
    }]);

    let package2 = ibis_ooxml::Package::open(saved2.clone()).unwrap();
    let xml2 = package2.xml("ppt/slides/slide1.xml").unwrap();
    let root2 = xml2.root_element().unwrap();
    let ppr2 = xml2.descendants(root2).into_iter().find(|&n| xml2.is_element(n) && xml2.local_name(n) == "pPr").expect("pPr");
    let tab_lst2 = xml2.descendants(ppr2).into_iter().find(|&n| xml2.is_element(n) && xml2.local_name(n) == "tabLst");
    assert!(tab_lst2.is_none(), "cleared tabs removes tabLst");

    // 3. Undo / Redo の確認
    assert!(s.document.undo());
    let re_undo = s.p().slides[0].shape(shape_id).unwrap().text_body().unwrap();
    assert_eq!(re_undo.paragraphs[0].margin_left, 457_200);
    assert_eq!(re_undo.paragraphs[0].indent, -228_600);
    assert_eq!(re_undo.paragraphs[0].tabs.len(), 3);

    assert!(s.document.redo());
    let re_redo = s.p().slides[0].shape(shape_id).unwrap().text_body().unwrap();
    assert_eq!(re_redo.paragraphs[0].margin_left, 0);
    assert_eq!(re_redo.paragraphs[0].indent, 0);
    assert_eq!(re_redo.paragraphs[0].tabs.len(), 0);

    // 4. 新規テキストボックスの作成・保存・再読込
    let new_id = s.document.ids().new_shape_id();
    let mut new_text_body = ibis_doc_model::TextBody::plain("インデント段落", ibis_doc_model::TextStyle::default());
    new_text_body.paragraphs[0].margin_left = 360_000;
    new_text_body.paragraphs[0].indent = 180_000;
    new_text_body.paragraphs[0].tabs = vec![
        ibis_doc_model::TextTabStop { position: 1_000_000, align: ibis_doc_model::TextTabAlign::Center },
    ];
    let new_shape = ibis_doc_model::Shape::new(
        new_id,
        ibis_doc_model::EmuRect::new(1_000_000, 1_000_000, 4_000_000, 2_000_000),
        ibis_doc_model::ShapeKind::Auto(ibis_doc_model::AutoShape::text_box(ibis_doc_model::Fill::None, new_text_body)),
    );
    let saved_new = s.run(vec![Command::InsertShape {
        slide: slide_id,
        index: s.p().slides[0].shapes.len(),
        shape: new_shape,
    }]);

    let package_new = ibis_ooxml::Package::open(saved_new.clone()).unwrap();
    let xml_new = package_new.xml("ppt/slides/slide1.xml").unwrap();
    let root_new = xml_new.root_element().unwrap();
    let pprs: Vec<_> = xml_new.descendants(root_new).into_iter().filter(|&n| xml_new.is_element(n) && xml_new.local_name(n) == "pPr" && xml_new.attr(n, "marL") == Some("360000")).collect();
    assert!(!pprs.is_empty(), "new shape has pPr with marL=360000");
    assert_eq!(xml_new.attr(pprs[0], "indent"), Some("180000"));
    let new_tab_lst = xml_new.descendants(pprs[0]).into_iter().find(|&n| xml_new.is_element(n) && xml_new.local_name(n) == "tabLst").expect("new shape tabLst");
    let new_tabs: Vec<_> = xml_new.descendants(new_tab_lst).into_iter().filter(|&n| xml_new.is_element(n) && xml_new.local_name(n) == "tab").collect();
    assert_eq!(new_tabs.len(), 1);
    assert_eq!(xml_new.attr(new_tabs[0], "pos"), Some("1000000"));
    assert_eq!(xml_new.attr(new_tabs[0], "algn"), Some("c"));

    let reopened_new = ibis_pptx::open(saved_new).unwrap();
    let re_new_body = reopened_new.presentation.slides[0].shapes.last().unwrap().text_body().unwrap();
    assert_eq!(re_new_body.paragraphs[0].margin_left, 360_000);
    assert_eq!(re_new_body.paragraphs[0].indent, 180_000);
    assert_eq!(re_new_body.paragraphs[0].tabs.len(), 1);
    assert_eq!(re_new_body.paragraphs[0].tabs[0].position, 1_000_000);
    assert_eq!(re_new_body.paragraphs[0].tabs[0].align, ibis_doc_model::TextTabAlign::Center);
}

#[test]
fn save_slideshow_ppsx() {
    assert!(ibis_pptx::is_slideshow("ppsx"));
    assert!(ibis_pptx::is_slideshow("PPSX"));
    assert!(ibis_pptx::is_slideshow("ppsm"));
    assert!(!ibis_pptx::is_slideshow("pptx"));
    assert_eq!(ibis_pptx::Kind::from_extension("ppsx"), Some(ibis_pptx::Kind::Presentation));
    assert_eq!(ibis_pptx::Kind::from_extension("ppsm"), Some(ibis_pptx::Kind::Presentation));

    let data = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/pptx-plan.pptx")).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();

    // 1. .ppsx (スライドショー形式) として保存
    let ppsx_bytes = opened.source.save_slideshow(&opened.presentation, SaveOptions::default()).unwrap();
    let ppsx_pkg = ibis_ooxml::Package::open(ppsx_bytes.clone()).unwrap();
    let ct = ppsx_pkg.content_type("ppt/presentation.xml").unwrap();
    assert_eq!(
        ct.as_deref(),
        Some("application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml")
    );
    let ct_xml = String::from_utf8(ppsx_pkg.read("[Content_Types].xml").unwrap()).unwrap();
    assert!(ct_xml.contains("application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml"));

    // 2. 保存された .ppsx を開いて内容が同一であることを確認
    let ppsx_opened = ibis_pptx::open(ppsx_bytes).unwrap();
    assert_eq!(ppsx_opened.presentation.slides.len(), opened.presentation.slides.len());
    for (orig_s, ppsx_s) in opened.presentation.slides.iter().zip(&ppsx_opened.presentation.slides) {
        assert_eq!(orig_s.shapes.len(), ppsx_s.shapes.len());
    }

    // 3. .ppsx から通常 .pptx として保存した場合は presentation.main+xml に戻る
    let pptx_bytes = ppsx_opened.source.save_presentation(&ppsx_opened.presentation, SaveOptions::default()).unwrap();
    let pptx_pkg = ibis_ooxml::Package::open(pptx_bytes).unwrap();
    assert_eq!(
        pptx_pkg.content_type("ppt/presentation.xml").unwrap().as_deref(),
        Some("application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml")
    );
}

#[test]
fn save_template_potx() {
    assert!(ibis_pptx::is_template("potx"));
    assert!(ibis_pptx::is_template("POTX"));
    assert!(ibis_pptx::is_template("potm"));
    assert!(!ibis_pptx::is_template("pptx"));
    assert_eq!(ibis_pptx::Kind::from_extension("potx"), Some(ibis_pptx::Kind::Presentation));
    assert_eq!(ibis_pptx::Kind::from_extension("potm"), Some(ibis_pptx::Kind::Presentation));

    let data = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/pptx-plan.pptx")).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();

    // 1. .potx (テンプレート形式) として保存
    let potx_bytes = opened.source.save_template(&opened.presentation, SaveOptions::default()).unwrap();
    let potx_pkg = ibis_ooxml::Package::open(potx_bytes.clone()).unwrap();
    let ct = potx_pkg.content_type("ppt/presentation.xml").unwrap();
    assert_eq!(
        ct.as_deref(),
        Some("application/vnd.openxmlformats-officedocument.presentationml.template.main+xml")
    );
    let ct_xml = String::from_utf8(potx_pkg.read("[Content_Types].xml").unwrap()).unwrap();
    assert!(ct_xml.contains("application/vnd.openxmlformats-officedocument.presentationml.template.main+xml"));

    // 2. 保存された .potx を開いて内容が同一であることを確認
    let potx_opened = ibis_pptx::open(potx_bytes).unwrap();
    assert_eq!(potx_opened.presentation.slides.len(), opened.presentation.slides.len());
    for (orig_s, potx_s) in opened.presentation.slides.iter().zip(&potx_opened.presentation.slides) {
        assert_eq!(orig_s.shapes.len(), potx_s.shapes.len());
    }

    // 3. .potx から通常 .pptx として保存した場合は presentation.main+xml に戻る
    let pptx_bytes = potx_opened.source.save_presentation(&potx_opened.presentation, SaveOptions::default()).unwrap();
    let pptx_pkg = ibis_ooxml::Package::open(pptx_bytes).unwrap();
    assert_eq!(
        pptx_pkg.content_type("ppt/presentation.xml").unwrap().as_deref(),
        Some("application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml")
    );
}

#[test]
fn save_theme_thmx_round_trip() {
    let data = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/pptx-plan.pptx")).unwrap();
    let opened = ibis_pptx::open(data).unwrap();

    // 1. .thmx (Office テーマ形式) として保存
    let thmx_bytes = opened.source.save_theme(&opened.presentation, Some("Ibis Custom Theme")).unwrap();
    assert!(!thmx_bytes.is_empty());

    // 2. OOXML パッケージ構造の検証
    let thmx_pkg = ibis_ooxml::Package::open(thmx_bytes).unwrap();
    let ct = thmx_pkg.content_type("theme/theme/theme1.xml").unwrap();
    assert_eq!(
        ct.as_deref(),
        Some("application/vnd.openxmlformats-officedocument.theme+xml")
    );
    let mgr_ct = thmx_pkg.content_type("theme/theme/themeManager.xml").unwrap();
    assert_eq!(
        mgr_ct.as_deref(),
        Some("application/vnd.openxmlformats-officedocument.themeManager+xml")
    );

    // 3. テーマ XML の中身を検証
    let theme_xml = String::from_utf8(thmx_pkg.read("theme/theme/theme1.xml").unwrap()).unwrap();
    assert!(theme_xml.contains("Ibis Custom Theme"));
    assert!(theme_xml.contains("<a:theme"));
    assert!(theme_xml.contains("<a:clrScheme"));
}


/// 最終版としてマーク（Mark as Final）および読み取り専用を推奨（ReadOnlyRecommended）の
/// 設定・保存・OOXML検証・再読込・解除のラウンドトリップ検証。
#[test]
fn test_mark_as_final_and_read_only_round_trip() {
    let bytes = std::fs::read(test_file("benchmark/earnings-presentation.pptx")).expect("read earnings-presentation.pptx");
    let opened = ibis_pptx::open(bytes).expect("open earnings-presentation");
    assert!(!opened.presentation.mark_as_final);
    assert!(!opened.presentation.read_only_recommended);

    // 1. 両フラグを有効にして保存
    let mut modified = opened.presentation.clone();
    modified.mark_as_final = true;
    modified.read_only_recommended = true;

    let saved_bytes = opened
        .source
        .save_presentation(&modified, SaveOptions::default())
        .expect("save presentation with flags");

    // 2. OOXML パッケージ構造の検証
    let pkg = ibis_ooxml::Package::open(saved_bytes.clone()).expect("open saved package");
    let custom_xml = String::from_utf8(pkg.read("docProps/custom.xml").expect("read custom.xml")).unwrap();
    assert!(custom_xml.contains("_MarkAsFinal"), "custom.xml should contain _MarkAsFinal");
    assert!(custom_xml.contains("<vt:bool>true</vt:bool>"), "custom.xml should contain vt:bool true");

    let pres_xml = String::from_utf8(pkg.read("ppt/presentation.xml").expect("read presentation.xml")).unwrap();
    assert!(pres_xml.contains("readonlyRecommended"), "presentation.xml should contain readonlyRecommended");

    // 3. 再読込での値保持検証
    let reopened = ibis_pptx::open(saved_bytes).expect("reopen saved presentation");
    assert!(reopened.presentation.mark_as_final);
    assert!(reopened.presentation.read_only_recommended);

    // 4. フラグを解除して再保存
    let mut cleared = reopened.presentation.clone();
    cleared.mark_as_final = false;
    cleared.read_only_recommended = false;

    let cleared_bytes = reopened
        .source
        .save_presentation(&cleared, SaveOptions::default())
        .expect("save cleared presentation");

    let reopened_cleared = ibis_pptx::open(cleared_bytes.clone()).expect("reopen cleared presentation");
    assert!(!reopened_cleared.presentation.mark_as_final);
    assert!(!reopened_cleared.presentation.read_only_recommended);

    let cleared_pkg = ibis_ooxml::Package::open(cleared_bytes).expect("open cleared package");
    if let Ok(cxml_bytes) = cleared_pkg.read("docProps/custom.xml") {
        let cxml = String::from_utf8(cxml_bytes).unwrap();
        assert!(!cxml.contains("_MarkAsFinal"), "custom.xml should not contain _MarkAsFinal");
    }
    let pres_xml_cleared = String::from_utf8(cleared_pkg.read("ppt/presentation.xml").unwrap()).unwrap();
    assert!(!pres_xml_cleared.contains("readonlyRecommended"), "presentation.xml should not contain readonlyRecommended");
}

/// ドキュメント プロパティ（Core: title, creator, subject, keywords, description等、
/// App: company, manager等、Custom: テキスト・数値・ブール値等）の設定・保存・
/// OOXMLパッケージ検証・再読込・更新のラウンドトリップ検証（PPTX-FILE-008, PPTX-FILE-009）。
#[test]
fn test_presentation_properties_and_custom_properties_round_trip() {
    use ibis_doc_model::{CustomProperty, CustomPropertyValue};

    let bytes = std::fs::read(test_file("benchmark/earnings-presentation.pptx")).expect("read earnings-presentation.pptx");
    let opened = ibis_pptx::open(bytes).expect("open presentation");

    // 1. プロパティを設定
    let mut modified = opened.presentation.clone();
    modified.properties.title = Some("決算説明会 2026年度".to_string());
    modified.properties.creator = Some("アイビス開発チーム".to_string());
    modified.properties.subject = Some("第3四半期決算".to_string());
    modified.properties.keywords = Some("pptx, presentation, benchmark".to_string());
    modified.properties.description = Some("プレゼンテーションのコメントと概要".to_string());
    modified.properties.company = Some("株式会社アイビス".to_string());
    modified.properties.manager = Some("開発責任者".to_string());
    modified.properties.custom = vec![
        CustomProperty {
            name: "部門".to_string(),
            value: CustomPropertyValue::Text("モバイル開発部".to_string()),
        },
        CustomProperty {
            name: "目標値".to_string(),
            value: CustomPropertyValue::Number("123.45".to_string()),
        },
        CustomProperty {
            name: "社外秘".to_string(),
            value: CustomPropertyValue::Bool(true),
        },
    ];

    // 2. 保存
    let saved_bytes = opened
        .source
        .save_presentation(&modified, SaveOptions::default())
        .expect("save presentation with properties");

    // 3. OOXML パッケージ構造の直接検証
    let pkg = ibis_ooxml::Package::open(saved_bytes.clone()).expect("open saved package");

    let core_xml = String::from_utf8(pkg.read("docProps/core.xml").expect("read core.xml")).unwrap();
    assert!(core_xml.contains("決算説明会 2026年度"), "core.xml has title");
    assert!(core_xml.contains("アイビス開発チーム"), "core.xml has creator");
    assert!(core_xml.contains("第3四半期決算"), "core.xml has subject");
    assert!(core_xml.contains("pptx, presentation, benchmark"), "core.xml has keywords");
    assert!(core_xml.contains("プレゼンテーションのコメントと概要"), "core.xml has description");

    let app_xml = String::from_utf8(pkg.read("docProps/app.xml").expect("read app.xml")).unwrap();
    assert!(app_xml.contains("株式会社アイビス"), "app.xml has company");
    assert!(app_xml.contains("開発責任者"), "app.xml has manager");

    let custom_xml = String::from_utf8(pkg.read("docProps/custom.xml").expect("read custom.xml")).unwrap();
    assert!(custom_xml.contains("name=\"部門\""), "custom.xml has 部門");
    assert!(custom_xml.contains("モバイル開発部"), "custom.xml has 部門 value");
    assert!(custom_xml.contains("name=\"目標値\""), "custom.xml has 目標値");
    assert!(custom_xml.contains("123.45"), "custom.xml has 目標値 value");
    assert!(custom_xml.contains("name=\"社外秘\""), "custom.xml has 社外秘");
    assert!(custom_xml.contains("<vt:bool>true</vt:bool>"), "custom.xml has boolean value");

    // 4. 再読込での値保持検証
    let reopened = ibis_pptx::open(saved_bytes).expect("reopen presentation");
    assert_eq!(reopened.presentation.properties.title.as_deref(), Some("決算説明会 2026年度"));
    assert_eq!(reopened.presentation.properties.creator.as_deref(), Some("アイビス開発チーム"));
    assert_eq!(reopened.presentation.properties.subject.as_deref(), Some("第3四半期決算"));
    assert_eq!(reopened.presentation.properties.keywords.as_deref(), Some("pptx, presentation, benchmark"));
    assert_eq!(reopened.presentation.properties.description.as_deref(), Some("プレゼンテーションのコメントと概要"));
    assert_eq!(reopened.presentation.properties.company.as_deref(), Some("株式会社アイビス"));
    assert_eq!(reopened.presentation.properties.manager.as_deref(), Some("開発責任者"));
    assert_eq!(reopened.presentation.properties.custom.len(), 3);
    assert_eq!(reopened.presentation.properties.custom[0].name, "部門");
    assert_eq!(reopened.presentation.properties.custom[0].value, CustomPropertyValue::Text("モバイル開発部".to_string()));
    assert_eq!(reopened.presentation.properties.custom[1].name, "目標値");
    assert_eq!(reopened.presentation.properties.custom[1].value, CustomPropertyValue::Number("123.45".to_string()));
    assert_eq!(reopened.presentation.properties.custom[2].name, "社外秘");
    assert_eq!(reopened.presentation.properties.custom[2].value, CustomPropertyValue::Bool(true));
}

#[test]
fn test_shape_decorative_round_trip() {
    let mut s = Scenario::open("shape-decorative", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let shape = s.p().slides[0].shapes[0].id;

    // 1. 最初は装飾用としてマークされていない
    assert!(!s.p().slides[0].decorative_shapes.contains(&shape));

    // 2. 装飾用としてマークして保存
    let saved_bytes = s.run(vec![Command::SetShapeDecorative {
        slide,
        shape,
        decorative: true,
    }]);

    // 3. OOXML パッケージ構造の検証
    let pkg = ibis_ooxml::Package::open(saved_bytes.clone()).expect("open package");
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").expect("read slide1.xml")).unwrap();
    assert!(slide_xml.contains("{C183D7F6-B498-43B3-948B-1728B52AA6E4}"), "slide1.xml should contain decorative URI");
    assert!(slide_xml.contains("decorative"), "slide1.xml should contain decorative element");
    assert!(slide_xml.contains("val=\"1\""), "slide1.xml should contain val=1");

    // 4. 再読込での値保持検証
    let reopened = ibis_pptx::open(saved_bytes).expect("reopen presentation");
    assert!(reopened.presentation.slides[0].decorative_shapes.contains(&shape), "reopened presentation should have shape marked as decorative");

    // 5. 解除して保存
    let cleared_bytes = s.run(vec![Command::SetShapeDecorative {
        slide,
        shape,
        decorative: false,
    }]);

    let cleared_pkg = ibis_ooxml::Package::open(cleared_bytes.clone()).expect("open cleared package");
    let cleared_xml = String::from_utf8(cleared_pkg.read("ppt/slides/slide1.xml").expect("read cleared slide1.xml")).unwrap();
    assert!(!cleared_xml.contains("{C183D7F6-B498-43B3-948B-1728B52AA6E4}"), "cleared slide1.xml should not contain decorative URI");

    let reopened_cleared = ibis_pptx::open(cleared_bytes).expect("reopen cleared");
    assert!(!reopened_cleared.presentation.slides[0].decorative_shapes.contains(&shape), "cleared shape should not be decorative");

    // 6. Undo / Redo の検証
    assert!(s.document.undo());
    assert!(s.p().slides[0].decorative_shapes.contains(&shape), "undo restores decorative");

    assert!(s.document.redo());
    assert!(!s.p().slides[0].decorative_shapes.contains(&shape), "redo clears decorative");
}

#[test]
fn test_document_inspection_and_cleaning_round_trip() {
    use ibis_doc_model::flow::DocProperties;
    use ibis_doc_model::{CustomProperty, CustomPropertyValue};

    let mut s = Scenario::open("doc-inspect-test", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;

    // 1. プロパティおよびノートを設定
    let mut props = DocProperties::default();
    props.creator = Some("Yamada Taro".into());
    props.company = Some("Ibis Corporation".into());
    props.title = Some("Confidential Strategy".into());
    props.custom.push(CustomProperty {
        name: "SecurityLevel".into(),
        value: CustomPropertyValue::Text("Restricted".into()),
    });

    let notes_text = "Private presenter notes: do not share externally.";

    let data_with_info = s.run(vec![
        Command::SetDocProperties { properties: props },
        Command::SetNotes { slide, notes: notes_text.into() },
    ]);

    // 2. OOXML 内にプロパティとノートが存在することを確認
    let pkg_info = ibis_ooxml::Package::open(data_with_info.clone()).expect("open package");
    let core_xml = String::from_utf8(pkg_info.read("docProps/core.xml").unwrap()).unwrap();
    assert!(core_xml.contains("Yamada Taro"), "core.xml has creator");
    assert!(core_xml.contains("Confidential Strategy"), "core.xml has title");
    let custom_xml = String::from_utf8(pkg_info.read("docProps/custom.xml").unwrap()).unwrap();
    assert!(custom_xml.contains("SecurityLevel"), "custom.xml has custom property");
    assert!(custom_xml.contains("Restricted"), "custom.xml has Restricted value");

    // 3. 検査ロジックの判定検証（モデル上の検出）
    let reopened = ibis_pptx::open(data_with_info.clone()).unwrap();
    assert!(reopened.presentation.properties.creator.as_deref() == Some("Yamada Taro"));
    assert!(reopened.presentation.properties.custom.len() == 1);
    assert!(!reopened.presentation.slides[0].notes.is_empty());

    // 4. 個人情報プロパティとノートの除去を実行
    let cleaned_data = s.run(vec![
        Command::SetDocProperties { properties: DocProperties::default() },
        Command::SetNotes { slide, notes: String::new() },
    ]);

    // 5. OOXML パッケージ内で個人情報とノートが除去されたことを検証
    let pkg_cleaned = ibis_ooxml::Package::open(cleaned_data.clone()).expect("open cleaned");
    if let Ok(core_bytes) = pkg_cleaned.read("docProps/core.xml") {
        let core_clean = String::from_utf8(core_bytes).unwrap();
        assert!(!core_clean.contains("Yamada Taro"), "core.xml creator cleared");
        assert!(!core_clean.contains("Confidential Strategy"), "core.xml title cleared");
    }
    if let Ok(custom_bytes) = pkg_cleaned.read("docProps/custom.xml") {
        let custom_clean = String::from_utf8(custom_bytes).unwrap();
        assert!(!custom_clean.contains("SecurityLevel"), "custom.xml property cleared");
    }

    // 6. 再読込での完全なクリア状態を検証
    let reopened_clean = ibis_pptx::open(cleaned_data).unwrap();
    assert!(reopened_clean.presentation.properties.creator.is_none() || reopened_clean.presentation.properties.creator.as_deref() == Some(""));
    assert!(reopened_clean.presentation.properties.custom.is_empty());
    assert!(reopened_clean.presentation.slides[0].notes.is_empty());

    // 7. Undo / Redo の検証
    assert!(s.document.undo());
    assert!(s.p().properties.creator.as_deref() == Some("Yamada Taro"));
    assert_eq!(s.p().slides[0].notes, notes_text);

    assert!(s.document.redo());
    assert!(s.p().properties.creator.is_none() || s.p().properties.creator.as_deref() == Some(""));
    assert!(s.p().slides[0].notes.is_empty());
}

#[test]
fn insert_and_save_svg_picture_round_trip() {
    let mut s = Scenario::open("svg-picture", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let svg_content = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><circle cx="50" cy="50" r="40" fill="red"/></svg>"#;
    let media = s.document.ids().add_media(MediaFormat::Svg, svg_content.to_vec());
    let id = s.document.ids().new_shape_id();
    let frame = EmuRect::new(1_000_000, 1_000_000, 952_500, 952_500); // 100x100 pt
    let picture = ibis_doc_model::Picture {
        media,
        crop: Default::default(),
        stroke: None,
        corner: 0,
        duotone: Some((ibis_doc_model::Rgb::from_rgb(0, 0, 128), ibis_doc_model::Rgb::from_rgb(255, 200, 200))),
        clear: None,
        luminance: None,
        clip: None,
        shadow: None,
        reflection: None,
        soft_edge: None,
        alt_text: Default::default(),
        decorative: false,
        name: String::new(), geometry: None, effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None };
    let shape = Shape::new(id, frame, ShapeKind::Picture(picture));
    let index = s.p().slides[0].shapes.len();
    let saved = s.run(vec![Command::InsertShape { slide, index, shape }]);

    // 1. パッケージ内の検証: ppt/media/image*.svg と [Content_Types].xml
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let has_svg_media = pkg.names().any(|p| p.starts_with("ppt/media/") && p.ends_with(".svg"));
    assert!(has_svg_media, "ppt/media/image*.svg should exist in the package");
    let content_types = String::from_utf8(pkg.read(ibis_ooxml::package::CONTENT_TYPES).unwrap()).unwrap();
    assert!(content_types.contains(r#"Extension="svg""#) && content_types.contains(r#"ContentType="image/svg+xml""#), "svg content type should be registered");

    // 2. スライド XML 内の検証: asvg:svgBlip と duotone
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(slide_xml.contains("asvg:svgBlip"), "asvg:svgBlip should be present in slide XML");
    assert!(slide_xml.contains("http://schemas.microsoft.com/office/drawing/2016/SVG/main"), "asvg namespace should be declared");
    assert!(slide_xml.contains("a:duotone"), "duotone color effect should be preserved");

    // 3. 再読込での検証
    let reopened = ibis_pptx::open(saved).unwrap();
    let reopened_slide = &reopened.presentation.slides[0];
    let reopened_shape = &reopened_slide.shapes[index];
    let ShapeKind::Picture(reopened_pic) = &reopened_shape.kind else { panic!("expected picture") };
    assert_eq!(reopened_pic.duotone, Some((ibis_doc_model::Rgb::from_rgb(0, 0, 128), ibis_doc_model::Rgb::from_rgb(255, 200, 200))));
    let reopened_media = reopened.presentation.media(reopened_pic.media).expect("media exists");
    assert_eq!(reopened_media.format, MediaFormat::Svg);
    assert_eq!(reopened_media.data.as_ref(), svg_content);
}

#[test]
fn replace_existing_picture_with_svg() {
    let mut s = Scenario::open("replace-picture-svg", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let picture_shape = new_shapes(s.document.ids()).pop().unwrap();
    let index = s.p().slides[0].shapes.len();
    let saved = s.run(vec![Command::InsertShape { slide, index, shape: picture_shape }]);
    s.opened = ibis_pptx::open(saved.clone()).unwrap();
    s.data = saved;
    s.document = Document::new(s.opened.presentation.clone());
    let slide = s.p().slides[0].id;
    let original = s.p().slides[0].shapes[index].clone();

    // SVG に置換
    let svg_content = br#"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="60"><rect width="60" height="60" fill="blue"/></svg>"#;
    let svg_media = s.document.ids().add_media(MediaFormat::Svg, svg_content.to_vec());
    let ShapeKind::Picture(mut picture) = original.kind.clone() else { panic!("picture") };
    picture.media = svg_media;
    let saved2 = s.run(vec![Command::SetKind { slide, shape: original.id, kind: Box::new(ShapeKind::Picture(picture)) }]);

    let pkg = ibis_ooxml::Package::open(saved2.clone()).unwrap();
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(slide_xml.contains("asvg:svgBlip"), "replaced picture has asvg:svgBlip in slide XML");

    let reopened = ibis_pptx::open(saved2).unwrap();
    let reopened_slide = &reopened.presentation.slides[0];
    let reopened_shape = &reopened_slide.shapes[index];
    let ShapeKind::Picture(pic) = &reopened_shape.kind else { panic!("expected picture") };
    let media = reopened.presentation.media(pic.media).unwrap();
    assert_eq!(media.format, MediaFormat::Svg);
    assert_eq!(media.data.as_ref(), svg_content);
}

#[test]
fn hidden_slide_title_round_trip() {
    let mut s = Scenario::open("hidden-slide-title", "editing/pptx-plan.pptx");
    let slide_id = s.p().slides[0].id;
    let shape_len = s.p().slides[0].shapes.len();
    let size = s.p().size_of(&s.p().slides[0]);

    // 1. オフスクリーンのタイトルプレースホルダーとスライド名を設定
    let title = "非表示スライドの概要".to_string();
    let frame = EmuRect::new(0, -1_500_000, size.width, 800_000);
    let shape_id = s.document.ids().new_shape_id();
    let shape = Shape::text_box(shape_id, frame, &title);
    let ph = ibis_doc_model::SlidePlaceholder {
        placeholder: ibis_doc_model::PlaceholderRef { kind: Some("title".to_string()), index: None },
        own_frame: true,
    };
    s.document.execute(vec![
        Command::InsertShape { slide: slide_id, index: shape_len, shape },
        Command::SetPlaceholder { slide: slide_id, shape: shape_id, placeholder: Some(ph) },
        Command::SetSlideName { slide: slide_id, name: Some(title.clone()) },
    ]).unwrap();
    let saved = s.opened.source.save(s.p(), SaveOptions::default()).unwrap();

    // XML検証: p:cSld/@name とタイトルプレースホルダー
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(slide_xml.contains(&format!(r#"name="{}""#, title)), "p:cSld has name attribute");
    assert!(slide_xml.contains(r#"type="title""#), "shape has title placeholder type");

    // 再読込での検証
    let reopened = ibis_pptx::open(saved.clone()).unwrap();
    let reopened_slide = &reopened.presentation.slides[0];
    let reopened_slide_id = reopened_slide.id;
    assert_eq!(reopened_slide.name, Some(title));
    let reopened_shape = reopened_slide.shapes.iter().find(|s| s.frame.y == -1_500_000).expect("shape exists");
    let reopened_ph = reopened_slide.placeholders.get(&reopened_shape.id);
    assert!(reopened_ph.is_some_and(|p| p.placeholder.normalized() == "title"));

    // 2. スライド名の変更（Scenario の run で undo/redo/save round-trip を完全検証）
    let mut s2 = Scenario {
        name: format!("{}-updated", s.name),
        data: saved.clone(),
        document: Document::new(reopened.presentation.clone()),
        opened: reopened,
    };
    let new_title = "更新された非表示タイトル".to_string();
    let saved2 = s2.run(vec![Command::SetSlideName { slide: reopened_slide_id, name: Some(new_title.clone()) }]);

    let pkg2 = ibis_ooxml::Package::open(saved2.clone()).unwrap();
    let slide_xml2 = String::from_utf8(pkg2.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(slide_xml2.contains(&format!(r#"name="{}""#, new_title)), "p:cSld updated name attribute");

    let reopened2 = ibis_pptx::open(saved2.clone()).unwrap();
    assert_eq!(reopened2.presentation.slides[0].name, Some(new_title));
    let reopened2_slide_id = reopened2.presentation.slides[0].id;

    // 3. スライド名の削除 (None)（Scenario の run で完全検証）
    let mut s3 = Scenario {
        name: format!("{}-removed", s.name),
        data: saved2.clone(),
        document: Document::new(reopened2.presentation.clone()),
        opened: reopened2,
    };
    let saved3 = s3.run(vec![Command::SetSlideName { slide: reopened2_slide_id, name: None }]);

    let pkg3 = ibis_ooxml::Package::open(saved3.clone()).unwrap();
    let slide_xml3 = pkg3.read("ppt/slides/slide1.xml").unwrap();
    let doc3 = ibis_ooxml::XmlDocument::parse(&slide_xml3).unwrap();
    let root3 = doc3.root_element().unwrap();
    let csld3 = doc3.child(root3, "cSld").unwrap();
    assert!(doc3.attr_local(csld3, "name").is_none(), "p:cSld name attribute removed");

    let reopened3 = ibis_pptx::open(saved3).unwrap();
    assert_eq!(reopened3.presentation.slides[0].name, None);
}

#[test]
fn test_proofing_language_round_trip() {
    let mut s = Scenario::open("proofing-language-round-trip", "editing/pptx-plan.pptx");
    let (slide, shape, _) = text_shapes(s.p(), 1).into_iter().next().unwrap();

    // 1. "en-US" を設定
    let saved = s.run(vec![Command::SetCharacterFormat {
        slide,
        shape,
        start: 0,
        end: 2,
        format: ibis_doc_model::CharacterFormat {
            lang: Some(Some("en-US".into())),
            ..Default::default()
        },
    }]);

    let reopened = ibis_pptx::open(saved.clone()).unwrap();
    let body = reopened.presentation.slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap();
    let run0 = &body.paragraphs[0].runs[0];
    assert_eq!(run0.style.lang.as_deref(), Some("en-US"), "run0 lang should be en-US");

    let pkg = ibis_ooxml::Package::open(saved).unwrap();
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(slide_xml.contains(r#"lang="en-US""#), "XML should contain lang=\"en-US\"");

    // 2. "fr-FR" を設定
    let saved2 = s.run(vec![Command::SetCharacterFormat {
        slide,
        shape,
        start: 0,
        end: 2,
        format: ibis_doc_model::CharacterFormat {
            lang: Some(Some("fr-FR".into())),
            ..Default::default()
        },
    }]);

    let reopened2 = ibis_pptx::open(saved2.clone()).unwrap();
    let body2 = reopened2.presentation.slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap();
    assert_eq!(body2.paragraphs[0].runs[0].style.lang.as_deref(), Some("fr-FR"), "run0 lang should be fr-FR");

    // 3. Undo して en-US に戻る
    assert!(s.document.undo());
    let saved_undo = s.opened.source.save(s.p(), Default::default()).unwrap();
    let reopened_undo = ibis_pptx::open(saved_undo).unwrap();
    let body_undo = reopened_undo.presentation.slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap();
    assert_eq!(body_undo.paragraphs[0].runs[0].style.lang.as_deref(), Some("en-US"), "undo should restore en-US");

    // 4. Redo して fr-FR に戻る
    assert!(s.document.redo());
    let saved_redo = s.opened.source.save(s.p(), Default::default()).unwrap();
    let reopened_redo = ibis_pptx::open(saved_redo).unwrap();
    let body_redo = reopened_redo.presentation.slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap();
    assert_eq!(body_redo.paragraphs[0].runs[0].style.lang.as_deref(), Some("fr-FR"), "redo should restore fr-FR");

    // 5. 言語をクリア（None）
    let saved_clear = s.run(vec![Command::SetCharacterFormat {
        slide,
        shape,
        start: 0,
        end: 2,
        format: ibis_doc_model::CharacterFormat {
            lang: Some(None),
            ..Default::default()
        },
    }]);

    let reopened_clear = ibis_pptx::open(saved_clear).unwrap();
    let body_clear = reopened_clear.presentation.slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap();
    assert_eq!(body_clear.paragraphs[0].runs[0].style.lang, None, "cleared lang should be None");
}

#[test]
fn test_transition_sound_round_trip() {
    let mut s = Scenario::open("transition-sound-round-trip", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;

    // 1. 効果音 "Applause" (loop: false) を設定
    let trans = ibis_doc_model::Transition {
        sound: Some(ibis_doc_model::TransitionSound::Play {
            name: "Applause".into(),
            loop_sound: false,
        }),
        ..ibis_doc_model::Transition::new("fade")
    };
    let saved = s.run(vec![Command::SetTransition { slide, transition: Some(trans) }]);

    let reopened = ibis_pptx::open(saved.clone()).unwrap();
    let t = reopened.presentation.slide(slide).unwrap().transition.as_ref().unwrap();
    assert_eq!(
        t.sound,
        Some(ibis_doc_model::TransitionSound::Play {
            name: "Applause".into(),
            loop_sound: false,
        }),
        "reopened transition sound should be Applause"
    );

    let pkg = ibis_ooxml::Package::open(saved).unwrap();
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(slide_xml.contains("sndAc"), "XML should contain sndAc");
    assert!(slide_xml.contains(r#"name="Applause""#), "XML should contain snd name Applause");

    // 2. loop: true に更新
    let trans_loop = ibis_doc_model::Transition {
        sound: Some(ibis_doc_model::TransitionSound::Play {
            name: "Applause".into(),
            loop_sound: true,
        }),
        ..ibis_doc_model::Transition::new("fade")
    };
    let saved_loop = s.run(vec![Command::SetTransition { slide, transition: Some(trans_loop) }]);

    let reopened_loop = ibis_pptx::open(saved_loop.clone()).unwrap();
    let t_loop = reopened_loop.presentation.slide(slide).unwrap().transition.as_ref().unwrap();
    assert_eq!(
        t_loop.sound,
        Some(ibis_doc_model::TransitionSound::Play {
            name: "Applause".into(),
            loop_sound: true,
        }),
        "reopened transition sound should be looped"
    );

    let pkg_loop = ibis_ooxml::Package::open(saved_loop).unwrap();
    let slide_xml_loop = String::from_utf8(pkg_loop.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(slide_xml_loop.contains(r#"loop="1""#), "XML should contain loop=\"1\"");

    // 3. Stop Sound を設定
    let trans_stop = ibis_doc_model::Transition {
        sound: Some(ibis_doc_model::TransitionSound::Stop),
        ..ibis_doc_model::Transition::new("fade")
    };
    let saved_stop = s.run(vec![Command::SetTransition { slide, transition: Some(trans_stop) }]);

    let reopened_stop = ibis_pptx::open(saved_stop.clone()).unwrap();
    let t_stop = reopened_stop.presentation.slide(slide).unwrap().transition.as_ref().unwrap();
    assert_eq!(
        t_stop.sound,
        Some(ibis_doc_model::TransitionSound::Stop),
        "reopened transition sound should be Stop"
    );

    let pkg_stop = ibis_ooxml::Package::open(saved_stop).unwrap();
    let slide_xml_stop = String::from_utf8(pkg_stop.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(slide_xml_stop.contains("endSnd"), "XML should contain endSnd");

    // 4. Undo して loop: true に戻る
    assert!(s.document.undo());
    let saved_undo = s.opened.source.save(s.p(), Default::default()).unwrap();
    let reopened_undo = ibis_pptx::open(saved_undo).unwrap();
    assert_eq!(
        reopened_undo.presentation.slide(slide).unwrap().transition.as_ref().unwrap().sound,
        Some(ibis_doc_model::TransitionSound::Play {
            name: "Applause".into(),
            loop_sound: true,
        })
    );

    // 5. Redo して Stop に戻る
    assert!(s.document.redo());
    let saved_redo = s.opened.source.save(s.p(), Default::default()).unwrap();
    let reopened_redo = ibis_pptx::open(saved_redo).unwrap();
    assert_eq!(
        reopened_redo.presentation.slide(slide).unwrap().transition.as_ref().unwrap().sound,
        Some(ibis_doc_model::TransitionSound::Stop)
    );

    // 6. sound: None（サウンドなし）に設定
    let trans_none = ibis_doc_model::Transition {
        sound: None,
        ..ibis_doc_model::Transition::new("fade")
    };
    let saved_none = s.run(vec![Command::SetTransition { slide, transition: Some(trans_none) }]);

    let reopened_none = ibis_pptx::open(saved_none.clone()).unwrap();
    let t_none = reopened_none.presentation.slide(slide).unwrap().transition.as_ref().unwrap();
    assert_eq!(t_none.sound, None, "reopened transition sound should be None");

    let pkg_none = ibis_ooxml::Package::open(saved_none).unwrap();
    let slide_xml_none = String::from_utf8(pkg_none.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(!slide_xml_none.contains("sndAc"), "XML should not contain sndAc");
}

#[test]
fn test_animation_repeat_and_autoreverse_round_trip() {
    let mut s = Scenario::open("anim-repeat-autorev", "editing/pptx-plan.pptx");
    let slide = s.p().slides[1].id;
    let shape = s.p().slides[1].shapes[0].id;

    // 1. アニメーションを追加: フェード (Entrance), repeat: Count(3), auto_reverse: true
    let mut anim = ibis_doc_model::Animation {
        shape,
        class: ibis_doc_model::AnimationClass::Entrance,
        start: ibis_doc_model::AnimationStart::OnClick,
        look: ibis_doc_model::AnimationLook::Fade,
        delay: 500,
        duration: 1000,
        preset: (10, 0),
        repeat_count: ibis_doc_model::AnimationRepeat::Count(3),
        auto_reverse: true,
        rewind: false,
        accel: 0,
        decel: 0,
        after_effect: ibis_doc_model::AnimationAfterEffect::Hold,
        trigger: ibis_doc_model::AnimationTrigger::None,
        paragraph: None, chars: None, text_unit: None, path: Vec::new(),
        chart_element: None, chart_build: None,
        dgm_element: None, dgm_build: None,
    };
    let saved = s.run(vec![Command::SetAnimations { slide, animations: vec![anim.clone()] }]);

    // 再読込して検証
    let reopened = ibis_pptx::open(saved.clone()).unwrap();
    let a = &reopened.presentation.slide(slide).unwrap().animations[0];
    assert_eq!(a.repeat_count, ibis_doc_model::AnimationRepeat::Count(3));
    assert!(a.auto_reverse);

    // XML に repeatCount="3000" と autoRev="1" が出力されていることを検証
    let pkg = ibis_ooxml::Package::open(saved).unwrap();
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide2.xml").unwrap()).unwrap();
    assert!(slide_xml.contains("repeatCount=\"3000\""), "slide XML should contain repeatCount=\"3000\"");
    assert!(slide_xml.contains("autoRev=\"1\""), "slide XML should contain autoRev=\"1\"");

    // 2. repeat: UntilNextClick, auto_reverse: false に変更
    anim.repeat_count = ibis_doc_model::AnimationRepeat::UntilNextClick;
    anim.auto_reverse = false;
    let saved_click = s.run(vec![Command::SetAnimations { slide, animations: vec![anim.clone()] }]);

    let reopened_click = ibis_pptx::open(saved_click.clone()).unwrap();
    let a_click = &reopened_click.presentation.slide(slide).unwrap().animations[0];
    assert_eq!(a_click.repeat_count, ibis_doc_model::AnimationRepeat::UntilNextClick);
    assert!(!a_click.auto_reverse);

    let pkg_click = ibis_ooxml::Package::open(saved_click).unwrap();
    let slide_xml_click = String::from_utf8(pkg_click.read("ppt/slides/slide2.xml").unwrap()).unwrap();
    assert!(slide_xml_click.contains("repeatCount=\"indefinite\""), "slide XML should contain repeatCount=\"indefinite\"");
    assert!(!slide_xml_click.contains("autoRev="), "slide XML should not contain autoRev");

    // 3. Undo して Count(3) & auto_reverse: true に戻る
    assert!(s.document.undo());
    let saved_undo = s.opened.source.save(s.p(), Default::default()).unwrap();
    let reopened_undo = ibis_pptx::open(saved_undo).unwrap();
    let a_undo = &reopened_undo.presentation.slide(slide).unwrap().animations[0];
    assert_eq!(a_undo.repeat_count, ibis_doc_model::AnimationRepeat::Count(3));
    assert!(a_undo.auto_reverse);

    // 4. Redo して UntilNextClick に戻る
    assert!(s.document.redo());
    let saved_redo = s.opened.source.save(s.p(), Default::default()).unwrap();
    let reopened_redo = ibis_pptx::open(saved_redo).unwrap();
    let a_redo = &reopened_redo.presentation.slide(slide).unwrap().animations[0];
    assert_eq!(a_redo.repeat_count, ibis_doc_model::AnimationRepeat::UntilNextClick);
    assert!(!a_redo.auto_reverse);

    // 5. repeat: None (繰り返しなし) に変更
    anim.repeat_count = ibis_doc_model::AnimationRepeat::None;
    let saved_none = s.run(vec![Command::SetAnimations { slide, animations: vec![anim.clone()] }]);

    let reopened_none = ibis_pptx::open(saved_none.clone()).unwrap();
    let a_none = &reopened_none.presentation.slide(slide).unwrap().animations[0];
    assert_eq!(a_none.repeat_count, ibis_doc_model::AnimationRepeat::None);

    let pkg_none = ibis_ooxml::Package::open(saved_none).unwrap();
    let slide_xml_none = String::from_utf8(pkg_none.read("ppt/slides/slide2.xml").unwrap()).unwrap();
    assert!(!slide_xml_none.contains("repeatCount="), "slide XML should not contain repeatCount");
}

#[test]
fn test_animation_smooth_start_and_end_round_trip() {
    let mut s = Scenario::open("anim-smooth", "editing/pptx-plan.pptx");
    let slide = s.p().slides[1].id;
    let shape = s.p().slides[1].shapes[0].id;

    // 1. アニメーションを追加: フェード (Entrance), accel: 50000, decel: 50000 (両方滑らか)
    let mut anim = ibis_doc_model::Animation {
        shape,
        class: ibis_doc_model::AnimationClass::Entrance,
        start: ibis_doc_model::AnimationStart::OnClick,
        look: ibis_doc_model::AnimationLook::Fade,
        delay: 500,
        duration: 1000,
        preset: (10, 0),
        repeat_count: ibis_doc_model::AnimationRepeat::None,
        auto_reverse: false,
        rewind: false,
        accel: 50_000,
        decel: 50_000,
        after_effect: ibis_doc_model::AnimationAfterEffect::Hold,
        trigger: ibis_doc_model::AnimationTrigger::None,
        paragraph: None, chars: None, text_unit: None, path: Vec::new(),
        chart_element: None, chart_build: None,
        dgm_element: None, dgm_build: None,
    };
    let saved = s.run(vec![Command::SetAnimations { slide, animations: vec![anim.clone()] }]);

    // 再読込して検証
    let reopened = ibis_pptx::open(saved.clone()).unwrap();
    let a = &reopened.presentation.slide(slide).unwrap().animations[0];
    assert_eq!(a.accel, 50_000);
    assert_eq!(a.decel, 50_000);
    assert!(a.smooth_start());
    assert!(a.smooth_end());

    // XML に accel="50000" と decel="50000" が出力されていることを検証
    let pkg = ibis_ooxml::Package::open(saved).unwrap();
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide2.xml").unwrap()).unwrap();
    assert!(slide_xml.contains("accel=\"50000\""), "slide XML should contain accel=\"50000\"");
    assert!(slide_xml.contains("decel=\"50000\""), "slide XML should contain decel=\"50000\"");

    // 2. accel: 50000, decel: 0 (滑らかに開始のみ) に変更
    anim.accel = 50_000;
    anim.decel = 0;
    let saved_start = s.run(vec![Command::SetAnimations { slide, animations: vec![anim.clone()] }]);

    let reopened_start = ibis_pptx::open(saved_start.clone()).unwrap();
    let a_start = &reopened_start.presentation.slide(slide).unwrap().animations[0];
    assert_eq!(a_start.accel, 50_000);
    assert_eq!(a_start.decel, 0);
    assert!(a_start.smooth_start());
    assert!(!a_start.smooth_end());

    let pkg_start = ibis_ooxml::Package::open(saved_start).unwrap();
    let slide_xml_start = String::from_utf8(pkg_start.read("ppt/slides/slide2.xml").unwrap()).unwrap();
    assert!(slide_xml_start.contains("accel=\"50000\""), "slide XML should contain accel=\"50000\"");
    assert!(!slide_xml_start.contains("decel="), "slide XML should not contain decel");

    // 3. Undo / Redo で状態が復元されることを検証
    assert!(s.document.undo());
    assert_eq!(s.p().slide(slide).unwrap().animations[0].accel, 50_000);
    assert_eq!(s.p().slide(slide).unwrap().animations[0].decel, 50_000);

    assert!(s.document.redo());
    assert_eq!(s.p().slide(slide).unwrap().animations[0].accel, 50_000);
    assert_eq!(s.p().slide(slide).unwrap().animations[0].decel, 0);

    // 4. accel: 0, decel: 0 (等速) に変更
    anim.accel = 0;
    anim.decel = 0;
    let saved_none = s.run(vec![Command::SetAnimations { slide, animations: vec![anim.clone()] }]);

    let reopened_none = ibis_pptx::open(saved_none.clone()).unwrap();
    let a_none = &reopened_none.presentation.slide(slide).unwrap().animations[0];
    assert_eq!(a_none.accel, 0);
    assert_eq!(a_none.decel, 0);
    assert!(!a_none.smooth_start());
    assert!(!a_none.smooth_end());

    let pkg_none = ibis_ooxml::Package::open(saved_none).unwrap();
    let slide_xml_none = String::from_utf8(pkg_none.read("ppt/slides/slide2.xml").unwrap()).unwrap();
    assert!(!slide_xml_none.contains("accel="), "slide XML should not contain accel");
    assert!(!slide_xml_none.contains("decel="), "slide XML should not contain decel");
}

#[test]
fn test_animation_after_effect_round_trip() {
    let mut s = Scenario::open("anim-after", "editing/pptx-plan.pptx");
    let slide = s.p().slides[1].id;
    let shape = s.p().slides[1].shapes[0].id;

    // 1. アニメーションを追加: フェード (Entrance), after_effect: Hide (再生後に非表示)
    let mut anim = ibis_doc_model::Animation {
        shape,
        class: ibis_doc_model::AnimationClass::Entrance,
        start: ibis_doc_model::AnimationStart::OnClick,
        look: ibis_doc_model::AnimationLook::Fade,
        delay: 500,
        duration: 1000,
        preset: (10, 0),
        repeat_count: ibis_doc_model::AnimationRepeat::None,
        auto_reverse: false,
        rewind: false,
        accel: 0,
        decel: 0,
        after_effect: ibis_doc_model::AnimationAfterEffect::Hide,
        trigger: ibis_doc_model::AnimationTrigger::None,
        paragraph: None, chars: None, text_unit: None, path: Vec::new(),
        chart_element: None, chart_build: None,
        dgm_element: None, dgm_build: None,
    };
    let saved = s.run(vec![Command::SetAnimations { slide, animations: vec![anim.clone()] }]);

    // 再読込して検証
    let reopened = ibis_pptx::open(saved.clone()).unwrap();
    let a = &reopened.presentation.slide(slide).unwrap().animations[0];
    assert_eq!(a.after_effect, ibis_doc_model::AnimationAfterEffect::Hide);

    // XML に fill="remove" が出力されていることを検証
    let pkg = ibis_ooxml::Package::open(saved).unwrap();
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide2.xml").unwrap()).unwrap();
    assert!(slide_xml.contains("fill=\"remove\""), "slide XML should contain fill=\"remove\"");

    // 2. after_effect: Hold (消去しない) に変更
    anim.after_effect = ibis_doc_model::AnimationAfterEffect::Hold;
    let saved_hold = s.run(vec![Command::SetAnimations { slide, animations: vec![anim.clone()] }]);

    let reopened_hold = ibis_pptx::open(saved_hold.clone()).unwrap();
    let a_hold = &reopened_hold.presentation.slide(slide).unwrap().animations[0];
    assert_eq!(a_hold.after_effect, ibis_doc_model::AnimationAfterEffect::Hold);

    let pkg_hold = ibis_ooxml::Package::open(saved_hold).unwrap();
    let slide_xml_hold = String::from_utf8(pkg_hold.read("ppt/slides/slide2.xml").unwrap()).unwrap();
    assert!(slide_xml_hold.contains("fill=\"hold\""), "slide XML should contain fill=\"hold\"");

    // 3. Undo / Redo で状態が復元されることを検証
    assert!(s.document.undo());
    assert_eq!(s.p().slide(slide).unwrap().animations[0].after_effect, ibis_doc_model::AnimationAfterEffect::Hide);

    assert!(s.document.redo());
    assert_eq!(s.p().slide(slide).unwrap().animations[0].after_effect, ibis_doc_model::AnimationAfterEffect::Hold);
}

#[test]
fn test_animation_rewind_round_trip() {
    let mut s = Scenario::open("anim-rewind", "editing/pptx-plan.pptx");
    let slide = s.p().slides[1].id;
    let shape = s.p().slides[1].shapes[0].id;

    // 1. アニメーションを追加: フェード (Entrance), rewind: true (再生終了後に巻き戻す)
    let mut anim = ibis_doc_model::Animation {
        shape,
        class: ibis_doc_model::AnimationClass::Entrance,
        start: ibis_doc_model::AnimationStart::OnClick,
        look: ibis_doc_model::AnimationLook::Fade,
        delay: 500,
        duration: 1000,
        preset: (10, 0),
        repeat_count: ibis_doc_model::AnimationRepeat::None,
        auto_reverse: false,
        rewind: true,
        accel: 0,
        decel: 0,
        after_effect: ibis_doc_model::AnimationAfterEffect::Hold,
        trigger: ibis_doc_model::AnimationTrigger::None,
        paragraph: None, chars: None, text_unit: None, path: Vec::new(),
        chart_element: None, chart_build: None,
        dgm_element: None, dgm_build: None,
    };
    let saved = s.run(vec![Command::SetAnimations { slide, animations: vec![anim.clone()] }]);

    // 再読込して検証
    let reopened = ibis_pptx::open(saved.clone()).unwrap();
    let a = &reopened.presentation.slide(slide).unwrap().animations[0];
    assert!(a.rewind);

    // XML に rewind="1" が出力されていることを検証
    let pkg = ibis_ooxml::Package::open(saved).unwrap();
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide2.xml").unwrap()).unwrap();
    assert!(slide_xml.contains("rewind=\"1\""), "slide XML should contain rewind=\"1\"");

    // 2. rewind: false に変更
    anim.rewind = false;
    let saved_no_rewind = s.run(vec![Command::SetAnimations { slide, animations: vec![anim.clone()] }]);

    let reopened_no = ibis_pptx::open(saved_no_rewind.clone()).unwrap();
    let a_no = &reopened_no.presentation.slide(slide).unwrap().animations[0];
    assert!(!a_no.rewind);

    let pkg_no = ibis_ooxml::Package::open(saved_no_rewind).unwrap();
    let slide_xml_no = String::from_utf8(pkg_no.read("ppt/slides/slide2.xml").unwrap()).unwrap();
    assert!(!slide_xml_no.contains("rewind="), "slide XML should not contain rewind");

    // 3. Undo / Redo で状態が復元されることを検証
    assert!(s.document.undo());
    assert!(s.p().slide(slide).unwrap().animations[0].rewind);

    assert!(s.document.redo());
    assert!(!s.p().slide(slide).unwrap().animations[0].rewind);
}

#[test]
fn test_animation_triggers_round_trip() {
    let mut s = Scenario::open("anim-triggers", "editing/pptx-plan.pptx");
    let slide = s.p().slides[1].id;
    let shape_target1 = s.p().slides[1].shapes[0].id;
    let shape_trigger = s.p().slides[1].shapes[1].id;

    // 1. 通常アニメーション + 図形クリックトリガー + メディアブックマークトリガー
    let anim_normal = ibis_doc_model::Animation {
        shape: shape_target1,
        class: ibis_doc_model::AnimationClass::Entrance,
        start: ibis_doc_model::AnimationStart::OnClick,
        look: ibis_doc_model::AnimationLook::Fade,
        delay: 0,
        duration: 500,
        preset: (10, 0),
        repeat_count: ibis_doc_model::AnimationRepeat::None,
        auto_reverse: false,
        rewind: false,
        accel: 0,
        decel: 0,
        after_effect: ibis_doc_model::AnimationAfterEffect::Hold,
        trigger: ibis_doc_model::AnimationTrigger::None,
        paragraph: None, chars: None, text_unit: None, path: Vec::new(),
        chart_element: None, chart_build: None,
        dgm_element: None, dgm_build: None,
    };
    let anim_click_trigger = ibis_doc_model::Animation {
        shape: shape_target1,
        class: ibis_doc_model::AnimationClass::Emphasis,
        start: ibis_doc_model::AnimationStart::OnClick,
        look: ibis_doc_model::AnimationLook::Spin(180_000),
        delay: 0,
        duration: 1000,
        preset: (8, 0),
        repeat_count: ibis_doc_model::AnimationRepeat::None,
        auto_reverse: false,
        rewind: false,
        accel: 0,
        decel: 0,
        after_effect: ibis_doc_model::AnimationAfterEffect::Hold,
        trigger: ibis_doc_model::AnimationTrigger::OnShapeClick(shape_trigger),
        paragraph: None, chars: None, text_unit: None, path: Vec::new(),
        chart_element: None, chart_build: None,
        dgm_element: None, dgm_build: None,
    };
    let anim_bmk_trigger = ibis_doc_model::Animation {
        shape: shape_trigger,
        class: ibis_doc_model::AnimationClass::Entrance,
        start: ibis_doc_model::AnimationStart::OnClick,
        look: ibis_doc_model::AnimationLook::Appear,
        delay: 200,
        duration: 1,
        preset: (1, 0),
        repeat_count: ibis_doc_model::AnimationRepeat::None,
        auto_reverse: false,
        rewind: false,
        accel: 0,
        decel: 0,
        after_effect: ibis_doc_model::AnimationAfterEffect::Hold,
        trigger: ibis_doc_model::AnimationTrigger::OnMediaBookmark {
            shape: shape_trigger,
            bookmark: "Bookmark 1".to_string(),
        },
        paragraph: None, chars: None, text_unit: None, path: Vec::new(),
        chart_element: None, chart_build: None,
        dgm_element: None, dgm_build: None,
    };

    let anims = vec![anim_normal.clone(), anim_click_trigger.clone(), anim_bmk_trigger.clone()];
    let saved = s.run(vec![Command::SetAnimations { slide, animations: anims.clone() }]);

    // 再読込して検証
    let reopened = ibis_pptx::open(saved.clone()).unwrap();
    let re_anims = &reopened.presentation.slide(slide).unwrap().animations;
    assert_eq!(re_anims.len(), 3);
    assert_eq!(re_anims[0].trigger, ibis_doc_model::AnimationTrigger::None);
    assert_eq!(re_anims[1].trigger, ibis_doc_model::AnimationTrigger::OnShapeClick(shape_trigger));
    assert_eq!(
        re_anims[2].trigger,
        ibis_doc_model::AnimationTrigger::OnMediaBookmark {
            shape: shape_trigger,
            bookmark: "Bookmark 1".to_string(),
        }
    );

    // XML に interactiveSeq, onClick, mediaBmKTrigger が出力されていることを検証
    let pkg = ibis_ooxml::Package::open(saved).unwrap();
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide2.xml").unwrap()).unwrap();
    assert!(slide_xml.contains("nodeType=\"interactiveSeq\""), "should contain interactiveSeq");
    assert!(slide_xml.contains("<p:cond evt=\"onClick\""), "should contain onClick condition");
    assert!(slide_xml.contains("p14:mediaBmKTrigger bmkName=\"Bookmark 1\""), "should contain mediaBmKTrigger");

    // 2. トリガーを解除して通常のシーケンスに戻す
    let mut anims_no_trigger = anims.clone();
    anims_no_trigger[1].trigger = ibis_doc_model::AnimationTrigger::None;
    anims_no_trigger[2].trigger = ibis_doc_model::AnimationTrigger::None;
    let saved_cleared = s.run(vec![Command::SetAnimations { slide, animations: anims_no_trigger.clone() }]);

    let reopened_cleared = ibis_pptx::open(saved_cleared.clone()).unwrap();
    let re_cleared = &reopened_cleared.presentation.slide(slide).unwrap().animations;
    assert_eq!(re_cleared.len(), 3);
    assert!(re_cleared.iter().all(|a| a.trigger.is_none()));

    let pkg_cleared = ibis_ooxml::Package::open(saved_cleared).unwrap();
    let slide_xml_cleared = String::from_utf8(pkg_cleared.read("ppt/slides/slide2.xml").unwrap()).unwrap();
    assert!(!slide_xml_cleared.contains("nodeType=\"interactiveSeq\""), "should not contain interactiveSeq after clearing");

    // 3. Undo / Redo で状態が復元されることを検証
    assert!(s.document.undo());
    assert_eq!(s.p().slide(slide).unwrap().animations[1].trigger, ibis_doc_model::AnimationTrigger::OnShapeClick(shape_trigger));
    assert_eq!(
        s.p().slide(slide).unwrap().animations[2].trigger,
        ibis_doc_model::AnimationTrigger::OnMediaBookmark {
            shape: shape_trigger,
            bookmark: "Bookmark 1".to_string(),
        }
    );

    assert!(s.document.redo());
    assert!(s.p().slide(slide).unwrap().animations.iter().all(|a| a.trigger.is_none()));
}

#[test]
fn test_chart_style_and_palette_round_trip() {
    let mut s = Scenario::open("chart-style-palette", "benchmark/earnings-presentation.pptx");
    let (slide, shape, chart) = s
        .p()
        .slides
        .iter()
        .find_map(|sl| sl.shapes.iter().find_map(|sh| match &sh.kind {
            ShapeKind::Chart(c) if c.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Bar { .. }) && !g.series.is_empty()) => Some((sl.id, sh.id, (**c).clone())),
            _ => None,
        }))
        .expect("a bar chart with series");

    let initial_style = chart.style;

    // 1. スタイルを 3 に変更して保存・再読込
    let mut modified = chart.clone();
    modified.style = Some(3);
    let mut p = s.p().clone();
    p.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(modified));
    let ShapeKind::Chart(read) = s.opened.source.as_read(&p, slide, shape).unwrap().kind else { panic!("a chart") };
    assert_eq!(read.style, Some(3));

    let saved1 = s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(read.clone())) }]);
    let reread1 = ibis_pptx::read(&saved1).unwrap();
    let ShapeKind::Chart(saved_chart1) = &reread1.slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("a chart") };
    assert_eq!(saved_chart1.style, Some(3));

    // XML を直接確認
    let pkg1 = ibis_ooxml::Package::open(saved1).unwrap();
    let has_style_3 = pkg1.names().filter(|n| n.starts_with("ppt/charts/chart") && !n.contains("style") && !n.contains("colors") && !n.contains(".rels")).any(|n| {
        let xml = String::from_utf8(pkg1.read(n).unwrap()).unwrap();
        xml.contains(r#"style val="3""#)
    });
    assert!(has_style_3, "XML should contain style 3");

    // 2. スタイルを 7 に変更して保存・再読込
    let mut modified2 = (*read).clone();
    modified2.style = Some(7);
    let mut p2 = s.p().clone();
    p2.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(modified2));
    let ShapeKind::Chart(read2) = s.opened.source.as_read(&p2, slide, shape).unwrap().kind else { panic!("a chart") };
    assert_eq!(read2.style, Some(7));

    let saved2 = s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(read2.clone())) }]);
    let reread2 = ibis_pptx::read(&saved2).unwrap();
    let ShapeKind::Chart(saved_chart2) = &reread2.slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("a chart") };
    assert_eq!(saved_chart2.style, Some(7));

    let pkg2 = ibis_ooxml::Package::open(saved2).unwrap();
    let has_style_7 = pkg2.names().filter(|n| n.starts_with("ppt/charts/chart") && !n.contains("style") && !n.contains("colors") && !n.contains(".rels")).any(|n| {
        let xml = String::from_utf8(pkg2.read(n).unwrap()).unwrap();
        xml.contains(r#"style val="7""#)
    });
    assert!(has_style_7, "XML should contain style 7");

    // 3. スタイルを None（スタイルなし）に変更して保存・再読込
    let mut modified3 = (*read2).clone();
    modified3.style = None;
    let mut p3 = s.p().clone();
    p3.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(modified3));
    let ShapeKind::Chart(read3) = s.opened.source.as_read(&p3, slide, shape).unwrap().kind else { panic!("a chart") };
    assert_eq!(read3.style, None);

    let saved3 = s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(read3.clone())) }]);
    let reread3 = ibis_pptx::read(&saved3).unwrap();
    let ShapeKind::Chart(saved_chart3) = &reread3.slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("a chart") };
    assert_eq!(saved_chart3.style, None);

    // 4. 配色パレットの適用（全系列の色を変更）して保存・再読込
    let mut modified4 = (*read3).clone();
    let palette_colors = [0x2F5597, 0xC55A11, 0x7B7B7B, 0xBF8F00, 0x1F4E79, 0x385723]; // カラフル 2
    let mut idx = 0;
    for ser in modified4.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
        let hex = palette_colors[idx % palette_colors.len()];
        ser.fill = Fill::Solid(Color::opaque(Rgb(hex)));
        idx += 1;
    }
    let mut p4 = s.p().clone();
    p4.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(modified4));
    let ShapeKind::Chart(read4) = s.opened.source.as_read(&p4, slide, shape).unwrap().kind else { panic!("a chart") };

    let saved4 = s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(read4.clone())) }]);
    let reread4 = ibis_pptx::read(&saved4).unwrap();
    let ShapeKind::Chart(saved_chart4) = &reread4.slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("a chart") };
    assert_eq!(saved_chart4.groups[0].series[0].fill, Fill::Solid(Color::opaque(Rgb(0x2F5597))));

    // 5. Undo / Redo でスタイルと色の復元を検証
    assert!(s.document.undo()); // back to style None (read3)
    let ShapeKind::Chart(ch_undo1) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("chart") };
    assert_eq!(ch_undo1.style, None);

    assert!(s.document.undo()); // back to style 7 (read2)
    let ShapeKind::Chart(ch_undo2) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("chart") };
    assert_eq!(ch_undo2.style, Some(7));

    assert!(s.document.undo()); // back to style 3 (read)
    let ShapeKind::Chart(ch_undo3) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("chart") };
    assert_eq!(ch_undo3.style, Some(3));

    assert!(s.document.undo()); // back to initial
    let ShapeKind::Chart(ch_undo4) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("chart") };
    assert_eq!(ch_undo4.style, initial_style);

    assert!(s.document.redo()); // style 3
    let ShapeKind::Chart(ch_redo1) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("chart") };
    assert_eq!(ch_redo1.style, Some(3));

    assert!(s.document.redo()); // style 7
    let ShapeKind::Chart(ch_redo2) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("chart") };
    assert_eq!(ch_redo2.style, Some(7));

    assert!(s.document.redo()); // style None
    let ShapeKind::Chart(ch_redo3) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("chart") };
    assert_eq!(ch_redo3.style, None);

    assert!(s.document.redo()); // palette
    let ShapeKind::Chart(ch_redo4) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("chart") };
    assert_eq!(ch_redo4.groups[0].series[0].fill, Fill::Solid(Color::opaque(Rgb(0x2F5597))));
}

#[test]
fn test_pie_and_doughnut_chart_type_and_settings_round_trip() {
    let mut s = Scenario::open("pie-and-doughnut", "benchmark/earnings-presentation.pptx");
    let (slide, shape, chart) = s
        .p()
        .slides
        .iter()
        .find_map(|sl| sl.shapes.iter().find_map(|sh| match &sh.kind {
            ShapeKind::Chart(c) if c.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Doughnut { .. })) => Some((sl.id, sh.id, (**c).clone())),
            _ => None,
        }))
        .expect("a doughnut chart");

    let initial_kind = chart.groups[0].kind;
    assert!(matches!(initial_kind, ibis_doc_model::ChartType::Doughnut { .. }));

    // 1. 穴の大きさを 50% に変更
    let mut modified1 = chart.clone();
    let ibis_doc_model::ChartType::Doughnut { first_angle, .. } = modified1.groups[0].kind else { panic!() };
    modified1.groups[0].kind = ibis_doc_model::ChartType::Doughnut { first_angle, hole: 50 };
    let mut p1 = s.p().clone();
    p1.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(modified1));
    let ShapeKind::Chart(read1) = s.opened.source.as_read(&p1, slide, shape).unwrap().kind else { panic!("a chart") };
    assert_eq!(read1.groups[0].kind, ibis_doc_model::ChartType::Doughnut { first_angle, hole: 50 });

    let saved1 = s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(read1.clone())) }]);
    let reread1 = ibis_pptx::read(&saved1).unwrap();
    let ShapeKind::Chart(saved_chart1) = &reread1.slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("a chart") };
    assert_eq!(saved_chart1.groups[0].kind, ibis_doc_model::ChartType::Doughnut { first_angle, hole: 50 });

    // XML に holeSize 50 が含まれること
    let pkg1 = ibis_ooxml::Package::open(saved1).unwrap();
    let has_hole_50 = pkg1.names().filter(|n| n.starts_with("ppt/charts/chart") && !n.contains(".rels")).any(|n| {
        let x = String::from_utf8(pkg1.read(n).unwrap()).unwrap();
        x.contains(r#"holeSize val="50""#)
    });
    assert!(has_hole_50, "XML should contain holeSize 50");

    // 2. 扇形の角度を 90° に変更
    let mut modified2 = (*read1).clone();
    modified2.groups[0].kind = ibis_doc_model::ChartType::Doughnut { first_angle: 90, hole: 50 };
    let mut p2 = s.p().clone();
    p2.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(modified2));
    let ShapeKind::Chart(read2) = s.opened.source.as_read(&p2, slide, shape).unwrap().kind else { panic!("a chart") };
    assert_eq!(read2.groups[0].kind, ibis_doc_model::ChartType::Doughnut { first_angle: 90, hole: 50 });

    let saved2 = s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(read2.clone())) }]);
    let reread2 = ibis_pptx::read(&saved2).unwrap();
    let ShapeKind::Chart(saved_chart2) = &reread2.slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("a chart") };
    assert_eq!(saved_chart2.groups[0].kind, ibis_doc_model::ChartType::Doughnut { first_angle: 90, hole: 50 });

    let pkg2 = ibis_ooxml::Package::open(saved2).unwrap();
    let has_first_slice_90 = pkg2.names().filter(|n| n.starts_with("ppt/charts/chart") && !n.contains(".rels")).any(|n| {
        let x = String::from_utf8(pkg2.read(n).unwrap()).unwrap();
        x.contains(r#"firstSliceAng val="90""#)
    });
    assert!(has_first_slice_90, "XML should contain firstSliceAng 90");

    // 3. 円グラフ (Pie) に種類を変更（角度90°を引き継ぐ）
    let mut modified3 = (*read2).clone();
    modified3.groups[0].kind = ibis_doc_model::ChartType::Pie { first_angle: 90 };
    let mut p3 = s.p().clone();
    p3.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == shape).unwrap().kind = ShapeKind::Chart(Box::new(modified3));
    let ShapeKind::Chart(read3) = s.opened.source.as_read(&p3, slide, shape).unwrap().kind else { panic!("a chart") };
    assert_eq!(read3.groups[0].kind, ibis_doc_model::ChartType::Pie { first_angle: 90 });

    let saved3 = s.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(read3.clone())) }]);
    let reread3 = ibis_pptx::read(&saved3).unwrap();
    let ShapeKind::Chart(saved_chart3) = &reread3.slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!("a chart") };
    assert_eq!(saved_chart3.groups[0].kind, ibis_doc_model::ChartType::Pie { first_angle: 90 });

    let pkg3 = ibis_ooxml::Package::open(saved3).unwrap();
    let has_pie_chart = pkg3.names().filter(|n| n.starts_with("ppt/charts/chart") && !n.contains(".rels")).any(|n| {
        let x = String::from_utf8(pkg3.read(n).unwrap()).unwrap();
        x.contains("pieChart") && x.contains(r#"firstSliceAng val="90""#) && !x.contains("holeSize")
    });
    assert!(has_pie_chart, "XML should contain pieChart with firstSliceAng 90 without holeSize");

    // 4. Undo/Redo の確認
    // Undo 3回: Pie -> Doughnut (angle 90, hole 50) -> Doughnut (angle 0, hole 50) -> initial
    assert!(s.document.undo());
    let ShapeKind::Chart(u1) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!() };
    assert_eq!(u1.groups[0].kind, ibis_doc_model::ChartType::Doughnut { first_angle: 90, hole: 50 });

    assert!(s.document.undo());
    let ShapeKind::Chart(u2) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!() };
    assert_eq!(u2.groups[0].kind, ibis_doc_model::ChartType::Doughnut { first_angle, hole: 50 });

    assert!(s.document.undo());
    let ShapeKind::Chart(u3) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!() };
    assert_eq!(u3.groups[0].kind, initial_kind);

    // Redo 3回: initial -> Doughnut (hole 50) -> Doughnut (angle 90, hole 50) -> Pie (angle 90)
    assert!(s.document.redo());
    let ShapeKind::Chart(r1) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!() };
    assert_eq!(r1.groups[0].kind, ibis_doc_model::ChartType::Doughnut { first_angle, hole: 50 });

    assert!(s.document.redo());
    let ShapeKind::Chart(r2) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!() };
    assert_eq!(r2.groups[0].kind, ibis_doc_model::ChartType::Doughnut { first_angle: 90, hole: 50 });

    assert!(s.document.redo());
    let ShapeKind::Chart(r3) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find(|sh| sh.id == shape).unwrap().kind else { panic!() };
    assert_eq!(r3.groups[0].kind, ibis_doc_model::ChartType::Pie { first_angle: 90 });
}

#[test]
fn test_scatter_and_bubble_charts_insert_save_and_convert() {
    use ibis_pptx::NewChartKind;
    use ibis_doc_model::{AxisKind, ChartType, Number, ShapeKind};

    let mut s = Scenario::open("scatter-and-bubble", "editing/pptx-plan.pptx");
    let slide = s.p().slides[1].id;

    // 1. 散布図（Scatter: マーカーのみ）の新規挿入
    let scatter_id = s.document.ids().new_shape_id();
    let current = s.p().clone();
    let scatter_shape = s
        .opened
        .source
        .new_chart_kind(
            &current,
            slide,
            scatter_id,
            EmuRect::new(1_000_000, 1_000_000, 5_000_000, 4_000_000),
            NewChartKind::Scatter,
        )
        .unwrap();
    let ShapeKind::Chart(scatter_chart) = &scatter_shape.kind else { panic!("a chart") };
    assert_eq!(scatter_chart.groups[0].kind, ChartType::Scatter { smooth: false });
    assert_eq!(scatter_chart.groups[0].series.len(), 1);
    assert_eq!(scatter_chart.groups[0].series[0].name, "系列 1");
    assert_eq!(scatter_chart.groups[0].series[0].x_values.len(), 4);
    assert_eq!(scatter_chart.groups[0].series[0].values.len(), 4);
    // 散布図は2つの数値軸（X軸とY軸）を持つ
    assert_eq!(scatter_chart.axes.len(), 2);
    assert!(scatter_chart.axes.iter().all(|a| a.kind == AxisKind::Value));

    let index = s.p().slides[1].shapes.len();
    let saved = s.run(vec![Command::InsertShape { slide, index, shape: scatter_shape.clone() }]);
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_slide = reread.slides.iter().find(|sl| sl.id == slide).unwrap();
    let reread_scatter = reread_slide.shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) if matches!(c.groups.first().map(|g| g.kind), Some(ChartType::Scatter { .. })) => Some(c),
        _ => None,
    }).expect("reread scatter chart");
    assert_eq!(reread_scatter.groups[0].kind, ChartType::Scatter { smooth: false });
    assert_eq!(reread_scatter.groups[0].series[0].x_values.len(), 4);
    assert_eq!(reread_scatter.groups[0].series[0].values.len(), 4);

    let pkg = ibis_ooxml::Package::open(saved).unwrap();
    let chart_parts: Vec<String> = pkg.names().filter(|n| n.starts_with("ppt/charts/chart") && n.ends_with(".xml")).map(|s| s.to_string()).collect();
    let scatter_xml = chart_parts.iter().find_map(|p| {
        let xml = String::from_utf8(pkg.read(p).unwrap()).unwrap();
        if xml.contains("scatterChart") { Some(xml) } else { None }
    }).expect("scatter chart xml");
    assert!(scatter_xml.contains(r#"scatterStyle val="marker""#));
    assert!(scatter_xml.contains("c:xVal"));
    assert!(scatter_xml.contains("c:yVal"));
    assert!(!scatter_xml.contains("c:catAx"));
    assert!(scatter_xml.matches("c:valAx").count() >= 2);

    // 2. 散布図（平滑線: ScatterSmooth）の新規挿入
    let smooth_id = s.document.ids().new_shape_id();
    let current2 = s.p().clone();
    let smooth_shape = s
        .opened
        .source
        .new_chart_kind(
            &current2,
            slide,
            smooth_id,
            EmuRect::new(6_000_000, 1_000_000, 5_000_000, 4_000_000),
            NewChartKind::ScatterSmooth,
        )
        .unwrap();
    let ShapeKind::Chart(smooth_chart) = &smooth_shape.kind else { panic!() };
    assert_eq!(smooth_chart.groups[0].kind, ChartType::Scatter { smooth: true });
    let index2 = s.p().slides[1].shapes.len();
    let saved_smooth = s.run(vec![Command::InsertShape { slide, index: index2, shape: smooth_shape.clone() }]);
    let pkg_smooth = ibis_ooxml::Package::open(saved_smooth).unwrap();
    let smooth_xml = pkg_smooth.names().find_map(|p| {
        if p.starts_with("ppt/charts/chart") && p.ends_with(".xml") {
            let xml = String::from_utf8(pkg_smooth.read(p).unwrap()).unwrap();
            if xml.contains(r#"scatterStyle val="smoothMarker""#) { Some(xml) } else { None }
        } else {
            None
        }
    }).expect("smooth marker xml");
    assert!(smooth_xml.contains("c:xVal"));

    // 3. バブルチャート（Bubble）の新規挿入
    let bubble_id = s.document.ids().new_shape_id();
    let current3 = s.p().clone();
    let bubble_shape = s
        .opened
        .source
        .new_chart_kind(
            &current3,
            slide,
            bubble_id,
            EmuRect::new(1_000_000, 5_500_000, 5_000_000, 4_000_000),
            NewChartKind::Bubble,
        )
        .unwrap();
    let ShapeKind::Chart(bubble_chart) = &bubble_shape.kind else { panic!() };
    assert_eq!(bubble_chart.groups[0].kind, ChartType::Bubble { scale: 100 });
    assert_eq!(bubble_chart.groups[0].series[0].x_values.len(), 4);
    assert_eq!(bubble_chart.groups[0].series[0].values.len(), 4);
    assert_eq!(bubble_chart.groups[0].series[0].bubble_sizes.len(), 4);
    assert_eq!(bubble_chart.axes.len(), 2);
    assert!(bubble_chart.axes.iter().all(|a| a.kind == AxisKind::Value));

    let index3 = s.p().slides[1].shapes.len();
    let saved_bubble = s.run(vec![Command::InsertShape { slide, index: index3, shape: bubble_shape.clone() }]);
    let reread_b = ibis_pptx::read(&saved_bubble).unwrap();
    let reread_b_slide = reread_b.slides.iter().find(|sl| sl.id == slide).unwrap();
    let reread_bubble = reread_b_slide.shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) if matches!(c.groups.first().map(|g| g.kind), Some(ChartType::Bubble { .. })) => Some(c),
        _ => None,
    }).expect("reread bubble chart");
    assert_eq!(reread_bubble.groups[0].kind, ChartType::Bubble { scale: 100 });
    assert_eq!(reread_bubble.groups[0].series[0].bubble_sizes.len(), 4);

    let pkg_b = ibis_ooxml::Package::open(saved_bubble).unwrap();
    let bubble_xml = pkg_b.names().find_map(|p| {
        if p.starts_with("ppt/charts/chart") && p.ends_with(".xml") {
            let xml = String::from_utf8(pkg_b.read(p).unwrap()).unwrap();
            if xml.contains("bubbleChart") { Some(xml) } else { None }
        } else {
            None
        }
    }).expect("bubble chart xml");
    assert!(bubble_xml.contains(r#"bubbleScale val="100""#));
    assert!(bubble_xml.contains("c:bubbleSize"));
    assert!(bubble_xml.contains("c:xVal"));
    assert!(bubble_xml.contains("c:yVal"));

    // 4. データ編集（x_values, values, bubble_sizes）と差分保存
    let mut modified_bubble = (**bubble_chart).clone();
    modified_bubble.groups[0].series[0].x_values[0] = Some(Number(10.5));
    modified_bubble.groups[0].series[0].values[0] = Some(Number(20.5));
    modified_bubble.groups[0].series[0].bubble_sizes[0] = Some(Number(99.0));

    let saved_data = s.run(vec![Command::SetKind { slide, shape: bubble_id, kind: Box::new(ShapeKind::Chart(Box::new(modified_bubble))) }]);
    let reread_data = ibis_pptx::read(&saved_data).unwrap();
    let reread_data_slide = reread_data.slides.iter().find(|sl| sl.id == slide).unwrap();
    let reread_data_chart = reread_data_slide.shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) if matches!(c.groups.first().map(|g| g.kind), Some(ChartType::Bubble { .. })) => Some(c),
        _ => None,
    }).expect("reread data chart");
    assert_eq!(reread_data_chart.groups[0].series[0].x_values[0], Some(Number(10.5)));
    assert_eq!(reread_data_chart.groups[0].series[0].values[0], Some(Number(20.5)));
    assert_eq!(reread_data_chart.groups[0].series[0].bubble_sizes[0], Some(Number(99.0)));

    let pkg_data = ibis_ooxml::Package::open(saved_data).unwrap();
    let data_xml = pkg_data.names().find_map(|p| {
        if p.starts_with("ppt/charts/chart") && p.ends_with(".xml") {
            let xml = String::from_utf8(pkg_data.read(p).unwrap()).unwrap();
            if xml.contains("99") && xml.contains("10.5") { Some(xml) } else { None }
        } else {
            None
        }
    }).expect("data update xml");
    assert!(data_xml.contains("<c:v>99</c:v>"));
    assert!(data_xml.contains("<c:v>10.5</c:v>"));

    // 5. 種類変更（バブル -> 散布図（平滑線） -> バブル（scale 200））
    let mut to_scatter = (**reread_data_chart).clone();
    to_scatter.groups[0].kind = ChartType::Scatter { smooth: true };
    let mut p_conv1 = s.p().clone();
    p_conv1.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == bubble_id).unwrap().kind = ShapeKind::Chart(Box::new(to_scatter));
    let ShapeKind::Chart(read_scatter) = s.opened.source.as_read(&p_conv1, slide, bubble_id).unwrap().kind else { panic!() };
    let saved_conv1 = s.run(vec![Command::SetKind { slide, shape: bubble_id, kind: Box::new(ShapeKind::Chart(read_scatter.clone())) }]);
    let reread_conv1 = ibis_pptx::read(&saved_conv1).unwrap();
    let conv1_chart = reread_conv1.slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) if matches!(c.groups.first().map(|g| g.kind), Some(ChartType::Scatter { smooth: true })) => Some(c),
        _ => None,
    }).expect("conv1 chart");
    assert_eq!(conv1_chart.groups[0].kind, ChartType::Scatter { smooth: true });

    let pkg_conv1 = ibis_ooxml::Package::open(saved_conv1).unwrap();
    let conv1_xml = pkg_conv1.names().find_map(|p| {
        if p.starts_with("ppt/charts/chart") && p.ends_with(".xml") {
            let xml = String::from_utf8(pkg_conv1.read(p).unwrap()).unwrap();
            if xml.contains(r#"scatterStyle val="smoothMarker""#) { Some(xml) } else { None }
        } else {
            None
        }
    }).expect("converted to scatter xml");
    assert!(!conv1_xml.contains("bubbleChart"));

    let mut to_bubble_scaled = (*read_scatter).clone();
    to_bubble_scaled.groups[0].kind = ChartType::Bubble { scale: 200 };
    let mut p_conv2 = s.p().clone();
    p_conv2.slides.iter_mut().find(|x| x.id == slide).unwrap().shapes.iter_mut().find(|x| x.id == bubble_id).unwrap().kind = ShapeKind::Chart(Box::new(to_bubble_scaled));
    let ShapeKind::Chart(read_bubble2) = s.opened.source.as_read(&p_conv2, slide, bubble_id).unwrap().kind else { panic!() };
    let saved_conv2 = s.run(vec![Command::SetKind { slide, shape: bubble_id, kind: Box::new(ShapeKind::Chart(read_bubble2.clone())) }]);
    let reread_conv2 = ibis_pptx::read(&saved_conv2).unwrap();
    let conv2_chart = reread_conv2.slides.iter().find(|sl| sl.id == slide).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) if matches!(c.groups.first().map(|g| g.kind), Some(ChartType::Bubble { scale: 200 })) => Some(c),
        _ => None,
    }).expect("conv2 chart");
    assert_eq!(conv2_chart.groups[0].kind, ChartType::Bubble { scale: 200 });

    let pkg_conv2 = ibis_ooxml::Package::open(saved_conv2).unwrap();
    let conv2_xml = pkg_conv2.names().find_map(|p| {
        if p.starts_with("ppt/charts/chart") && p.ends_with(".xml") {
            let xml = String::from_utf8(pkg_conv2.read(p).unwrap()).unwrap();
            if xml.contains(r#"bubbleScale val="200""#) { Some(xml) } else { None }
        } else {
            None
        }
    }).expect("converted back to bubble with scale 200");
    assert!(conv2_xml.contains("bubbleChart"));

    // 6. Undo/Redo の確認
    assert!(s.document.undo()); // back to Scatter { smooth: true }
    let ShapeKind::Chart(u1) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shape(bubble_id).unwrap().kind else { panic!() };
    assert_eq!(u1.groups[0].kind, ChartType::Scatter { smooth: true });

    assert!(s.document.undo()); // back to data modified Bubble { scale: 100 }
    let ShapeKind::Chart(u2) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shape(bubble_id).unwrap().kind else { panic!() };
    assert_eq!(u2.groups[0].kind, ChartType::Bubble { scale: 100 });
    assert_eq!(u2.groups[0].series[0].bubble_sizes[0], Some(Number(99.0)));

    assert!(s.document.redo()); // forward to Scatter { smooth: true }
    let ShapeKind::Chart(r1) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shape(bubble_id).unwrap().kind else { panic!() };
    assert_eq!(r1.groups[0].kind, ChartType::Scatter { smooth: true });

    assert!(s.document.redo()); // forward to Bubble { scale: 200 }
    let ShapeKind::Chart(r2) = &s.p().slides.iter().find(|sl| sl.id == slide).unwrap().shape(bubble_id).unwrap().kind else { panic!() };
    assert_eq!(r2.groups[0].kind, ChartType::Bubble { scale: 200 });
}

#[test]
fn test_connectors_insert_save_and_round_trip() {
    let mut s = Scenario::open("connectors-round-trip", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;

    // 1. 各種コネクタの作成
    // (a) 直線（両方向矢印）
    let line_id = s.document.ids().new_shape_id();
    let mut line_stroke = Stroke::solid(Color::opaque(Rgb(0x4472c4)), 19_050);
    line_stroke.head = LineEnd::Triangle(ibis_doc_model::EndSize { width: 1, length: 1 });
    line_stroke.tail = LineEnd::Triangle(ibis_doc_model::EndSize { width: 1, length: 1 });
    let mut line_shape = Shape::new(line_id, EmuRect::new(1_000_000, 1_000_000, 2_000_000, 1_000_000), ShapeKind::Line(line_stroke));
    line_shape.flip_h = false;
    line_shape.flip_v = false;

    // (b) カギ線コネクタ（終点矢印）
    let bent_id = s.document.ids().new_shape_id();
    let mut bent_stroke = Stroke::solid(Color::opaque(Rgb(0xed7d31)), 25_400);
    bent_stroke.tail = LineEnd::Triangle(ibis_doc_model::EndSize { width: 1, length: 1 });
    let bent_geom = ibis_pptx::preset_geometry("bentConnector3").expect("bentConnector3 preset");
    let bent_auto = AutoShape::new(bent_geom, Fill::None, Some(bent_stroke.clone()));
    let mut bent_shape = Shape::new(bent_id, EmuRect::new(1_000_000, 2_500_000, 3_000_000, 2_000_000), ShapeKind::Auto(bent_auto));
    bent_shape.flip_h = true;
    bent_shape.flip_v = false;

    // (c) 曲線コネクタ（両方向矢印）
    let curved_id = s.document.ids().new_shape_id();
    let mut curved_stroke = Stroke::solid(Color::opaque(Rgb(0x70ad47)), 12_700);
    curved_stroke.head = LineEnd::Triangle(ibis_doc_model::EndSize { width: 1, length: 1 });
    curved_stroke.tail = LineEnd::Triangle(ibis_doc_model::EndSize { width: 1, length: 1 });
    let curved_geom = ibis_pptx::preset_geometry("curvedConnector3").expect("curvedConnector3 preset");
    let curved_auto = AutoShape::new(curved_geom, Fill::None, Some(curved_stroke));
    let mut curved_shape = Shape::new(curved_id, EmuRect::new(1_000_000, 5_000_000, 2_500_000, 1_500_000), ShapeKind::Auto(curved_auto));
    curved_shape.flip_h = false;
    curved_shape.flip_v = true;

    // 2. コネクタを挿入して保存
    let count = s.p().slides[0].shapes.len();
    let saved_bytes = s.run(vec![
        Command::InsertShape { slide, shape: line_shape, index: count },
        Command::InsertShape { slide, shape: bent_shape, index: count + 1 },
        Command::InsertShape { slide, shape: curved_shape, index: count + 2 },
    ]);

    // 3. OOXML パッケージ構造の検証
    let pkg = ibis_ooxml::Package::open(saved_bytes.clone()).expect("open package");
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").expect("read slide1.xml")).unwrap();
    assert!(slide_xml.contains("cxnSp"), "slide1.xml should contain cxnSp");
    assert!(slide_xml.contains(r#"prst="bentConnector3""#), "slide1.xml should contain bentConnector3");
    assert!(slide_xml.contains(r#"prst="curvedConnector3""#), "slide1.xml should contain curvedConnector3");
    assert!(slide_xml.contains("headEnd"), "slide1.xml should contain headEnd");
    assert!(slide_xml.contains("tailEnd"), "slide1.xml should contain tailEnd");

    // 4. 再読込での値保持検証
    let reopened = ibis_pptx::open(saved_bytes.clone()).expect("reopen presentation");
    let r_slide = &reopened.presentation.slides[0];

    // (a) 直線
    let r_line = r_slide.shapes.iter().find(|sh| matches!(sh.kind, ShapeKind::Line(_))).expect("r_line");
    match &r_line.kind {
        ShapeKind::Line(st) => {
            assert_eq!(st.width, 19_050);
            assert_eq!(st.head, LineEnd::Triangle(ibis_doc_model::EndSize { width: 1, length: 1 }));
            assert_eq!(st.tail, LineEnd::Triangle(ibis_doc_model::EndSize { width: 1, length: 1 }));
        }
        _ => panic!("expected Line shape"),
    }

    // (b) カギ線
    let r_bent = r_slide.shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Auto(a) if a.geometry.preset_name() == Some("bentConnector3"))).expect("r_bent");
    assert!(r_bent.flip_h);
    assert!(!r_bent.flip_v);
    match &r_bent.kind {
        ShapeKind::Auto(a) => {
            assert_eq!(a.geometry.preset_name(), Some("bentConnector3"));
            let st = a.stroke.as_ref().expect("bent stroke");
            assert_eq!(st.width, 25_400);
            assert_eq!(st.head, LineEnd::None);
            assert_eq!(st.tail, LineEnd::Triangle(ibis_doc_model::EndSize { width: 1, length: 1 }));
        }
        _ => panic!("expected Auto shape for bent connector"),
    }

    // (c) 曲線
    let r_curved = r_slide.shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Auto(a) if a.geometry.preset_name() == Some("curvedConnector3"))).expect("r_curved");
    assert!(!r_curved.flip_h);
    assert!(r_curved.flip_v);
    match &r_curved.kind {
        ShapeKind::Auto(a) => {
            assert_eq!(a.geometry.preset_name(), Some("curvedConnector3"));
            let st = a.stroke.as_ref().expect("curved stroke");
            assert_eq!(st.width, 12_700);
            assert_eq!(st.head, LineEnd::Triangle(ibis_doc_model::EndSize { width: 1, length: 1 }));
            assert_eq!(st.tail, LineEnd::Triangle(ibis_doc_model::EndSize { width: 1, length: 1 }));
        }
        _ => panic!("expected Auto shape for curved connector"),
    }

    // 5. カギ線コネクタの線の太さ・破線・色を変更して再保存
    let updated_stroke = Stroke {
        color: Color::opaque(Rgb(0xff0000)),
        width: 38_100,
        dash: ibis_doc_model::LineDash::Dash,
        dashed: true,
        tail: LineEnd::Triangle(ibis_doc_model::EndSize { width: 2, length: 2 }),
        ..bent_stroke
    };
    let saved_modified = s.run(vec![
        Command::SetStroke { slide, shape: bent_id, stroke: Some(updated_stroke) },
    ]);
    let pkg_mod = ibis_ooxml::Package::open(saved_modified.clone()).expect("open modified package");
    let mod_xml = String::from_utf8(pkg_mod.read("ppt/slides/slide1.xml").expect("read modified slide1.xml")).unwrap();
    assert!(mod_xml.contains(r#"w="38100""#));
    assert!(mod_xml.contains("prstDash"));

    let reopened_mod = ibis_pptx::open(saved_modified).expect("reopen modified");
    let mod_bent = reopened_mod.presentation.slides[0].shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Auto(a) if a.geometry.preset_name() == Some("bentConnector3"))).expect("mod bent");
    match &mod_bent.kind {
        ShapeKind::Auto(a) => {
            let st = a.stroke.as_ref().expect("mod stroke");
            assert_eq!(st.width, 38_100);
            assert_eq!(st.color, Color::opaque(Rgb(0xff0000)));
            assert_eq!(st.dash, ibis_doc_model::LineDash::Dash);
            assert!(st.dashed);
        }
        _ => panic!("expected Auto shape"),
    }

    // 6. Undo / Redo
    assert!(s.document.undo());
    let ShapeKind::Auto(u_auto) = &s.p().slides[0].shape(bent_id).unwrap().kind else { panic!() };
    assert_eq!(u_auto.stroke.as_ref().unwrap().width, 25_400);

    assert!(s.document.redo());
    let ShapeKind::Auto(r_auto) = &s.p().slides[0].shape(bent_id).unwrap().kind else { panic!() };
    assert_eq!(r_auto.stroke.as_ref().unwrap().width, 38_100);
}

#[test]
fn test_connector_glue_and_follow_round_trip() {
    let mut s = Scenario::open("connectors-glue-round-trip", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;

    // 1. 2つの図形（四角形 rect_a と round_rect_b）を作成
    let a_id = s.document.ids().new_shape_id();
    let a_frame = EmuRect::new(100_000, 100_000, 1_000_000, 600_000);
    let a_geom = ibis_pptx::preset_geometry("rect").expect("rect preset");
    let a_auto = AutoShape::new(a_geom, Fill::Solid(Color::opaque(Rgb(0x4472c4))), Some(Stroke::solid(Color::opaque(Rgb(0x000000)), 12_700)));
    let a_shape = Shape::new(a_id, a_frame, ShapeKind::Auto(a_auto));

    let b_id = s.document.ids().new_shape_id();
    let b_frame = EmuRect::new(4_000_000, 2_000_000, 1_000_000, 600_000);
    let b_geom = ibis_pptx::preset_geometry("roundRect").expect("roundRect preset");
    let b_auto = AutoShape::new(b_geom, Fill::Solid(Color::opaque(Rgb(0xed7d31))), Some(Stroke::solid(Color::opaque(Rgb(0x000000)), 12_700)));
    let b_shape = Shape::new(b_id, b_frame, ShapeKind::Auto(b_auto));

    // 2. カギ線コネクタ（bentConnector3）を a の右接続点 (3) から b の左接続点 (1) に作成
    // aの右 (site 3): (1_100_000, 400_000), bの左 (site 1): (4_000_000, 2_300_000)
    let c_id = s.document.ids().new_shape_id();
    let (c_frame, c_fh, c_fv) = ibis_doc_model::connector::frame_between((1_100_000, 400_000), (4_000_000, 2_300_000));
    let mut c_stroke = Stroke::solid(Color::opaque(Rgb(0x70ad47)), 25_400);
    c_stroke.tail = LineEnd::Triangle(ibis_doc_model::EndSize { width: 1, length: 1 });
    let c_geom = ibis_pptx::preset_geometry("bentConnector3").expect("bentConnector3 preset");
    let c_auto = AutoShape::new(c_geom, Fill::None, Some(c_stroke));
    let mut c_shape = Shape::new(c_id, c_frame, ShapeKind::Auto(c_auto));
    c_shape.flip_h = c_fh;
    c_shape.flip_v = c_fv;

    let count = s.p().slides[0].shapes.len();
    let glue = ibis_doc_model::Glue {
        start: Some((a_id.0, 3)),
        end: Some((b_id.0, 1)),
    };
    let saved_bytes = s.run(vec![
        Command::InsertShape { slide, shape: a_shape, index: count },
        Command::InsertShape { slide, shape: b_shape, index: count + 1 },
        Command::InsertShape { slide, shape: c_shape, index: count + 2 },
        Command::SetShapeGlue { slide, shape: c_id, glue: Some(glue) },
    ]);

    // 3. OOXML XML の検証: slide1.xml に stCxn と endCxn が書き出されていること
    let pkg = ibis_ooxml::Package::open(saved_bytes.clone()).expect("open package");
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").expect("read slide1.xml")).unwrap();
    assert!(slide_xml.contains("<a:stCxn"), "slide1.xml should contain <a:stCxn");
    assert!(slide_xml.contains("<a:endCxn"), "slide1.xml should contain <a:endCxn");
    assert!(slide_xml.contains(r#"idx="3""#), "stCxn idx should be 3");
    assert!(slide_xml.contains(r#"idx="1""#), "endCxn idx should be 1");

    // 4. 再読込での保持検証
    let reopened = ibis_pptx::open(saved_bytes.clone()).expect("reopen presentation");
    let r_slide = &reopened.presentation.slides[0];
    let r_a = &r_slide.shapes[count];
    let r_b = &r_slide.shapes[count + 1];
    let r_c = &r_slide.shapes[count + 2];
    assert!(r_slide.shape_glues.contains_key(&r_c.id), "r_c glue must be preserved");
    let r_glue = r_slide.shape_glues.get(&r_c.id).unwrap();
    assert_eq!(r_glue.start, Some((r_a.id.0, 3)), "start glue preserved");
    assert_eq!(r_glue.end, Some((r_b.id.0, 1)), "end glue preserved");

    // 5. 図形 B を下方 (y: 2_000_000 -> 3_500_000) へ移動し、コネクタが追従
    let new_b_frame = EmuRect::new(4_000_000, 3_500_000, 1_000_000, 600_000);
    // 追従計算
    let mut updated_shapes = s.p().slides[0].shapes.clone();
    for sh in &mut updated_shapes {
        if sh.id == b_id {
            sh.frame = new_b_frame;
        }
    }
    let rerouted = ibis_doc_model::connector::rerouted_on_slide(&updated_shapes, &s.p().slides[0].shape_glues);
    assert_eq!(rerouted.len(), 1, "connector C should be rerouted");
    let (cid, new_c_frame, new_cfh, new_cfv) = rerouted[0];
    assert_eq!(cid, c_id);

    let saved_moved = s.run(vec![
        Command::SetFrame { slide, shape: b_id, frame: new_b_frame },
        Command::SetConnectorFrame { slide, shape: c_id, frame: new_c_frame, flip_h: new_cfh, flip_v: new_cfv },
    ]);

    // 移動後の再読込検証
    let reopened_moved = ibis_pptx::open(saved_moved).expect("reopen moved");
    let rm_slide = &reopened_moved.presentation.slides[0];
    let rm_a = &rm_slide.shapes[count];
    let rm_b = &rm_slide.shapes[count + 1];
    let rm_c = &rm_slide.shapes[count + 2];
    assert_eq!(rm_c.frame, new_c_frame, "connector frame updated");
    let rm_glue = rm_slide.shape_glues.get(&rm_c.id).unwrap();
    assert_eq!(rm_glue.start, Some((rm_a.id.0, 3)), "glue start preserved after move");
    assert_eq!(rm_glue.end, Some((rm_b.id.0, 1)), "glue end preserved after move");

    // 6. Undo/Redo の検証
    assert!(s.document.undo());
    assert_eq!(s.p().slides[0].shape(b_id).unwrap().frame, b_frame, "b frame restored on undo");
    assert_eq!(s.p().slides[0].shape(c_id).unwrap().frame, c_frame, "c frame restored on undo");

    assert!(s.document.redo());
    assert_eq!(s.p().slides[0].shape(b_id).unwrap().frame, new_b_frame, "b frame restored on redo");
    assert_eq!(s.p().slides[0].shape(c_id).unwrap().frame, new_c_frame, "c frame restored on redo");
}

#[test]
fn test_media_playback_options_round_trip() {
    use ibis_doc_model::{Clip, ClipOptions, MediaStart, MediaVolume};

    let mut s = Scenario::open("media-playback-options", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;

    // 1. メディア（動画・音声・ポスター画像）を登録
    let video_media = s.document.ids().add_media(MediaFormat::Mp4, vec![0x00, 0x00, 0x00, 0x18, 0x66, 0x74, 0x79, 0x70]);
    let audio_media = s.document.ids().add_media(MediaFormat::M4a, vec![0x00, 0x00, 0x00, 0x20, 0x66, 0x74, 0x79, 0x70]);
    let poster_media = s.document.ids().add_media(MediaFormat::Png, vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

    // 2. 動画図形（自動再生、音量低、ループあり、巻き戻しあり、全画面再生あり、停止時非表示）
    let video_clip = Clip {
        media: video_media,
        audio: false,
        options: ClipOptions {
            start: MediaStart::Automatically,
            volume: MediaVolume::Low,
            loop_until_stopped: true,
            rewind: true,
            full_screen: true,
            hide_while_not_playing: true,
            hide_during_show: false,
            play_across_slides: false,
            ..Default::default()
        },
        link: None,
    };
    let video_shape_id = s.document.ids().new_shape_id();
    let video_shape = Shape::new(
        video_shape_id,
        EmuRect::new(100_000, 100_000, 3_000_000, 2_000_000),
        ShapeKind::Picture(Picture {
            media: poster_media,
            crop: Default::default(),
            stroke: None,
            corner: 0,
            duotone: None,
            clear: None,
            luminance: None,
            clip: Some(video_clip),
            shadow: None,
            reflection: None,
            soft_edge: None,
            alt_text: Default::default(),
            decorative: false,
            name: "Video 1".to_string(), geometry: None, effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None }),
    );

    // 3. 音声図形（クリック時、音量ミュート、ループなし、巻き戻しなし、スライドショー実行中非表示、スライド切り替え後も再生）
    let audio_clip = Clip {
        media: audio_media,
        audio: true,
        options: ClipOptions {
            start: MediaStart::OnClick,
            volume: MediaVolume::Mute,
            loop_until_stopped: false,
            rewind: false,
            full_screen: false,
            hide_while_not_playing: false,
            hide_during_show: true,
            play_across_slides: true,
            ..Default::default()
        },
        link: None,
    };
    let audio_shape_id = s.document.ids().new_shape_id();
    let audio_shape = Shape::new(
        audio_shape_id,
        EmuRect::new(4_000_000, 100_000, 914_400, 914_400),
        ShapeKind::Picture(Picture {
            media: poster_media,
            crop: Default::default(),
            stroke: None,
            corner: 0,
            duotone: None,
            clear: None,
            luminance: None,
            clip: Some(audio_clip),
            shadow: None,
            reflection: None,
            soft_edge: None,
            alt_text: Default::default(),
            decorative: false,
            name: "Audio 1".to_string(), geometry: None, effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None }),
    );

    // 4. 図形を挿入して保存
    let count = s.p().slides[0].shapes.len();
    let saved_bytes = s.run(vec![
        Command::InsertShape { slide, index: count, shape: video_shape },
        Command::InsertShape { slide, index: count + 1, shape: audio_shape },
    ]);

    // 5. 保存した OOXML の XML 属性を検証
    let pkg = ibis_ooxml::Package::open(saved_bytes.clone()).expect("open package");
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").expect("read slide1.xml")).unwrap();
    // The options are those of the timing's media nodes, not attributes of `p14:media`.
    for stale in ["autoPlay=", "isNarration=", "playAcrossSlides=", "fullScreen=", "loop=", "rewind="] {
        assert!(!slide_xml.contains(stale), "{stale} is not an attribute of p14:media: {slide_xml}");
    }
    assert!(slide_xml.contains(r#"cmd="playFrom(0.0)""#), "play call expected: {slide_xml}");
    assert!(slide_xml.contains(r#"cmd="togglePause""#), "a video pauses by a click: {slide_xml}");
    assert!(slide_xml.contains(r#"<p:video fullScrn="1"><p:cMediaNode vol="33000" showWhenStopped="0">"#), "video node: {slide_xml}");
    assert!(slide_xml.contains(r#"repeatCount="indefinite""#), "loop expected in xml");
    assert!(slide_xml.contains(r#"fill="remove" display="0""#), "rewind expected in xml");
    assert!(slide_xml.contains(r#"nodeType="afterEffect""#), "automatic start expected in xml");
    assert!(slide_xml.contains(r#"<p:cMediaNode vol="0" numSld="999" showWhenStopped="0">"#), "audio node: {slide_xml}");
    assert!(!slide_xml.contains("onStopAudio"), "an audio played across slides is not stopped at the slide's end");

    // 6. 保存したプレゼンテーションを読み込み直して検証
    let reopened = ibis_pptx::open(saved_bytes).expect("reopen presentation");
    let r_slide = &reopened.presentation.slides[0];

    let r_video = r_slide.shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| !c.audio))).expect("video shape found");
    let ShapeKind::Picture(r_v_pic) = &r_video.kind else { panic!("expected picture") };
    let r_v_clip = r_v_pic.clip.as_ref().expect("expected video clip");
    assert_eq!(r_v_clip.audio, false);
    assert_eq!(r_v_clip.options.start, MediaStart::Automatically);
    assert_eq!(r_v_clip.options.volume, MediaVolume::Low);
    assert_eq!(r_v_clip.options.loop_until_stopped, true);
    assert_eq!(r_v_clip.options.rewind, true);
    assert_eq!(r_v_clip.options.full_screen, true);
    assert_eq!(r_v_clip.options.hide_while_not_playing, true);
    assert_eq!(r_v_clip.options.hide_during_show, false);

    let r_audio = r_slide.shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| c.audio))).expect("audio shape found");
    let ShapeKind::Picture(r_a_pic) = &r_audio.kind else { panic!("expected picture") };
    let r_a_clip = r_a_pic.clip.as_ref().expect("expected audio clip");
    assert_eq!(r_a_clip.audio, true);
    assert_eq!(r_a_clip.options.start, MediaStart::OnClick);
    assert_eq!(r_a_clip.options.volume, MediaVolume::Mute);
    assert_eq!(r_a_clip.options.loop_until_stopped, false);
    assert_eq!(r_a_clip.options.rewind, false);
    assert_eq!(r_a_clip.options.full_screen, false);
    assert_eq!(r_a_clip.options.hide_while_not_playing, false);
    assert_eq!(r_a_clip.options.hide_during_show, true);
    assert_eq!(r_a_clip.options.play_across_slides, true);

    // 7. オプションを変更して差分保存
    let mut updated_video_pic = match &s.p().slides[0].shape(video_shape_id).unwrap().kind {
        ShapeKind::Picture(p) => p.clone(),
        _ => panic!(),
    };
    if let Some(clip) = &mut updated_video_pic.clip {
        clip.options.start = MediaStart::OnClick;
        clip.options.volume = MediaVolume::High;
        clip.options.loop_until_stopped = false;
        clip.options.full_screen = false;
        clip.options.hide_while_not_playing = false;
    }
    // ポスター画像の変更も同時に検証（PPTX-VIDEO-012）
    let new_poster_media = s.document.ids().add_media(MediaFormat::Jpeg, vec![0xff, 0xd8, 0xff, 0xe0]);
    updated_video_pic.media = new_poster_media;

    let saved_mod = s.run(vec![
        Command::SetKind { slide, shape: video_shape_id, kind: Box::new(ShapeKind::Picture(updated_video_pic)) },
    ]);

    let reopened_mod = ibis_pptx::open(saved_mod).expect("reopen modified");
    let mod_video = reopened_mod.presentation.slides[0].shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| !c.audio))).expect("mod video shape");
    let ShapeKind::Picture(mod_v_pic) = &mod_video.kind else { panic!("expected picture") };
    let mod_v_clip = mod_v_pic.clip.as_ref().expect("expected video clip");
    assert_eq!(mod_v_clip.options.start, MediaStart::OnClick);
    assert_eq!(mod_v_clip.options.volume, MediaVolume::High);
    assert_eq!(mod_v_clip.options.loop_until_stopped, false);
    assert_eq!(mod_v_clip.options.full_screen, false);
    assert_eq!(mod_v_clip.options.hide_while_not_playing, false);

    // 8. Undo / Redo
    assert!(s.document.undo());
    let ShapeKind::Picture(u_pic) = &s.p().slides[0].shape(video_shape_id).unwrap().kind else { panic!() };
    assert_eq!(u_pic.clip.as_ref().unwrap().options.start, MediaStart::Automatically);
    assert_eq!(u_pic.clip.as_ref().unwrap().options.volume, MediaVolume::Low);
    assert_eq!(u_pic.clip.as_ref().unwrap().options.full_screen, true);
    assert_eq!(u_pic.clip.as_ref().unwrap().options.hide_while_not_playing, true);
    assert_eq!(u_pic.media, poster_media);

    assert!(s.document.redo());
    let ShapeKind::Picture(redo_pic) = &s.p().slides[0].shape(video_shape_id).unwrap().kind else { panic!() };
    assert_eq!(redo_pic.clip.as_ref().unwrap().options.start, MediaStart::OnClick);
    assert_eq!(redo_pic.clip.as_ref().unwrap().options.volume, MediaVolume::High);
    assert_eq!(redo_pic.clip.as_ref().unwrap().options.full_screen, false);
    assert_eq!(redo_pic.clip.as_ref().unwrap().options.hide_while_not_playing, false);
    assert_eq!(redo_pic.media, new_poster_media);
}

#[test]
fn test_object_defaults_round_trip() {
    let mut s = Scenario::open("object-defaults", "editing/pptx-plan.pptx");
    let initial_defaults = s.p().object_defaults.clone();

    // 1. 図形と線の既定スタイルを作成
    let shape_fill = Fill::Solid(Color::opaque(Rgb(0xff8000)));
    let shape_stroke = Stroke::solid(Color::opaque(Rgb(0x004080)), 25_400).with_dash(ibis_doc_model::LineDash::Dash);
    let shadow = ibis_doc_model::Shadow::simple(Color::opaque(Rgb(0x000000)), 50_000, 30_000, 5_400_000);
    let glow = (Color::opaque(Rgb(0xffc800)), 60_000);
    let reflection = ibis_doc_model::Reflection {
        blur: 10_000,
        start_alpha: 50_000,
        end_alpha: 300,
        start_pos: 0,
        end_pos: 100_000,
        distance: 20_000,
        direction: 5_400_000,
        fade_direction: 5_400_000,
        sx: 100_000,
        sy: -100_000,
        kx: 0,
        ky: 0,
        align: ibis_doc_model::TileAlign::Bottom,
        rot_with_shape: true,
    };
    let soft_edge = 40_000;

    let new_shape_default = ShapeDefault {
        fill: shape_fill.clone(),
        stroke: Some(shape_stroke.clone()),
        text: TextStyle {
            color: Rgb(0xffffff),
            font_size: 1800,
            bold: true,
            italic: false,
            ..Default::default()
        },
        shadow: Some(shadow),
        glow: Some(glow),
        reflection: Some(reflection),
        soft_edge: Some(soft_edge),
    };

    let new_line_default = Stroke::solid(Color::opaque(Rgb(0xc83232)), 38_100).with_dash(ibis_doc_model::LineDash::Dot);

    let new_defaults = ObjectDefaults {
        shape: Some(new_shape_default.clone()),
        line: Some(new_line_default.clone()),
    };

    // 2. Command::SetObjectDefaults で設定・Undo/Redo・保存
    let saved = s.run(vec![Command::SetObjectDefaults { defaults: new_defaults.clone() }]);

    // 3. 保存された pptx を再読込し、object_defaults が復元されていることを検証
    let reopened = ibis_pptx::open(saved.clone()).expect("reopen object_defaults pptx");
    let loaded_defaults = &reopened.presentation.object_defaults;
    assert_eq!(loaded_defaults.shape.as_ref().map(|sh| &sh.fill), Some(&new_shape_default.fill));
    assert_eq!(loaded_defaults.shape.as_ref().and_then(|sh| sh.stroke.as_ref()), new_shape_default.stroke.as_ref());
    assert_eq!(loaded_defaults.shape.as_ref().map(|sh| &sh.text.bold), Some(&true));
    assert_eq!(loaded_defaults.shape.as_ref().and_then(|sh| sh.shadow.as_ref()), new_shape_default.shadow.as_ref());
    assert_eq!(loaded_defaults.shape.as_ref().and_then(|sh| sh.glow), new_shape_default.glow);
    assert_eq!(loaded_defaults.shape.as_ref().and_then(|sh| sh.soft_edge), new_shape_default.soft_edge);
    assert_eq!(loaded_defaults.line.as_ref(), Some(&new_line_default));

    // 4. theme1.xml に <a:objectDefaults> 要素が出力されていることを OOXML パッケージレベルで検証
    let package = ibis_ooxml::Package::open(saved.clone()).expect("valid ooxml package");
    let theme_bytes = package.read("ppt/theme/theme1.xml").expect("theme1.xml found");
    let theme_xml = String::from_utf8(theme_bytes).expect("valid utf8 xml");
    assert!(theme_xml.contains("objectDefaults"), "theme contains objectDefaults");
    assert!(theme_xml.contains("spDef"), "theme contains spDef");
    assert!(theme_xml.contains("lnDef"), "theme contains lnDef");

    // 5. Undo で元に戻ることを検証
    assert!(s.document.undo());
    assert_eq!(s.p().object_defaults, initial_defaults);

    // 6. Redo で再度適用されることを検証
    assert!(s.document.redo());
    assert_eq!(s.p().object_defaults, new_defaults);
}

#[test]
fn test_action_buttons_and_links_round_trip() {
    use ibis_doc_model::SlideLink;
    let mut s = Scenario::open("action-links", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let shape1 = s.p().slides[0].shapes[0].id;
    let shape2 = s.p().slides[0].shapes[1].id;

    // 1. shape1 に LastSlideViewed、shape2 に EndShow を設定
    let saved = s.run(vec![
        Command::SetShapeLink { slide, shape: shape1, link: Some(SlideLink::LastSlideViewed) },
        Command::SetShapeLink { slide, shape: shape2, link: Some(SlideLink::EndShow) },
    ]);

    // 2. OOXML パッケージレベルで action="ppaction://hlinkshowjump?jump=lastslideviewed" および "jump=endshow" が保存されていることを検証
    let package = ibis_ooxml::Package::open(saved.clone()).expect("valid ooxml package");
    let slide_bytes = package.read("ppt/slides/slide1.xml").expect("slide1.xml found");
    let slide_xml = String::from_utf8(slide_bytes).expect("valid utf8 xml");
    assert!(slide_xml.contains("ppaction://hlinkshowjump?jump=lastslideviewed"), "slide contains lastslideviewed action link");
    assert!(slide_xml.contains("ppaction://hlinkshowjump?jump=endshow"), "slide contains endshow action link");

    // 3. 再オープンして、SlideLink::LastSlideViewed と SlideLink::EndShow が正しく復元されていることを検証
    let reopened = ibis_pptx::open(saved).expect("reopen action links pptx");
    assert_eq!(reopened.presentation.slides[0].shape_links.get(&shape1), Some(&SlideLink::LastSlideViewed));
    assert_eq!(reopened.presentation.slides[0].shape_links.get(&shape2), Some(&SlideLink::EndShow));

    // 4. Undo で元に戻ることを検証
    assert!(s.document.undo());
    assert_eq!(s.p().slides[0].shape_links.get(&shape1), None);
    assert_eq!(s.p().slides[0].shape_links.get(&shape2), None);

    // 5. Redo で再度適用されることを検証
    assert!(s.document.redo());
    assert_eq!(s.p().slides[0].shape_links.get(&shape1), Some(&SlideLink::LastSlideViewed));
    assert_eq!(s.p().slides[0].shape_links.get(&shape2), Some(&SlideLink::EndShow));
}

/// 動画・音声のトリミング（開始・終了時間: p14:trim）およびフェード（イン・アウト時間: p14:fade）の
/// 設定、OOXML差分保存、再オープン、OOXML要素検証、Undo/Redo、トリム/フェード解除を検証。
#[test]
fn test_media_trim_and_fade_options_round_trip() {
    use ibis_doc_model::{Clip, ClipOptions, MediaStart, MediaVolume};

    let mut s = Scenario::open("media-trim-fade", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;

    // 1. メディア（動画・音声・ポスター画像）を登録
    let video_media = s.document.ids().add_media(MediaFormat::Mp4, vec![0x00, 0x00, 0x00, 0x18, 0x66, 0x74, 0x79, 0x70]);
    let audio_media = s.document.ids().add_media(MediaFormat::M4a, vec![0x00, 0x00, 0x00, 0x20, 0x66, 0x74, 0x79, 0x70]);
    let poster_media = s.document.ids().add_media(MediaFormat::Png, vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

    // 2. 初期クリップ（トリム・フェードなし）
    let video_clip = Clip {
        media: video_media,
        audio: false,
        options: ClipOptions {
            start: MediaStart::Automatically,
            volume: MediaVolume::High,
            ..Default::default()
        },
        link: None,
    };
    let video_shape_id = s.document.ids().new_shape_id();
    let video_shape = Shape::new(
        video_shape_id,
        EmuRect::new(100_000, 100_000, 3_000_000, 2_000_000),
        ShapeKind::Picture(Picture {
            media: poster_media,
            crop: Default::default(),
            stroke: None,
            corner: 0,
            duotone: None,
            clear: None,
            luminance: None,
            clip: Some(video_clip),
            shadow: None,
            reflection: None,
            soft_edge: None,
            alt_text: Default::default(),
            decorative: false,
            name: "Video 1".to_string(), geometry: None, effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None }),
    );

    let audio_clip = Clip {
        media: audio_media,
        audio: true,
        options: ClipOptions {
            start: MediaStart::OnClick,
            volume: MediaVolume::Medium,
            ..Default::default()
        },
        link: None,
    };
    let audio_shape_id = s.document.ids().new_shape_id();
    let audio_shape = Shape::new(
        audio_shape_id,
        EmuRect::new(4_000_000, 100_000, 914_400, 914_400),
        ShapeKind::Picture(Picture {
            media: poster_media,
            crop: Default::default(),
            stroke: None,
            corner: 0,
            duotone: None,
            clear: None,
            luminance: None,
            clip: Some(audio_clip),
            shadow: None,
            reflection: None,
            soft_edge: None,
            alt_text: Default::default(),
            decorative: false,
            name: "Audio 1".to_string(), geometry: None, effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None }),
    );

    s.run(vec![
        Command::InsertShape { slide, index: 0, shape: video_shape },
        Command::InsertShape { slide, index: 1, shape: audio_shape },
    ]);

    // 3. 動画にトリミング（1.5s〜6.5s）とフェード（イン1.0s、アウト2.0s）を設定
    let mut updated_video = s.p().slides[0].shapes[0].clone();
    if let ShapeKind::Picture(ref mut pic) = updated_video.kind {
        if let Some(ref mut c) = pic.clip {
            c.options.trim_start_ms = 1500;
            c.options.trim_end_ms = 6500;
            c.options.fade_in_ms = 1000;
            c.options.fade_out_ms = 2000;
        }
    }

    // 4. 音声にトリミング（2.0s〜8.0s）とフェード（イン0.5s、アウト1.5s）を設定
    let mut updated_audio = s.p().slides[0].shapes[1].clone();
    if let ShapeKind::Picture(ref mut pic) = updated_audio.kind {
        if let Some(ref mut c) = pic.clip {
            c.options.trim_start_ms = 2000;
            c.options.trim_end_ms = 8000;
            c.options.fade_in_ms = 500;
            c.options.fade_out_ms = 1500;
        }
    }

    let saved = s.run(vec![
        Command::SetKind { slide, shape: video_shape_id, kind: Box::new(updated_video.kind.clone()) },
        Command::SetKind { slide, shape: audio_shape_id, kind: Box::new(updated_audio.kind.clone()) },
    ]);

    // 5. OOXML パッケージレベルで p14:trim および p14:fade が保存されていることを検証
    let package = ibis_ooxml::Package::open(saved.clone()).expect("valid ooxml package");
    let slide_bytes = package.read("ppt/slides/slide1.xml").expect("slide1.xml found");
    let slide_xml = String::from_utf8(slide_bytes).expect("valid utf8 xml");
    assert!(slide_xml.contains("p14:trim"), "contains p14:trim");
    assert!(slide_xml.contains("st=\"1500\""), "video trim start");
    assert!(slide_xml.contains("end=\"6500\""), "video trim end");
    assert!(slide_xml.contains("p14:fade"), "contains p14:fade");
    assert!(slide_xml.contains("in=\"1000\""), "video fade in");
    assert!(slide_xml.contains("out=\"2000\""), "video fade out");
    assert!(slide_xml.contains("st=\"2000\""), "audio trim start");
    assert!(slide_xml.contains("end=\"8000\""), "audio trim end");
    assert!(slide_xml.contains("in=\"500\""), "audio fade in");
    assert!(slide_xml.contains("out=\"1500\""), "audio fade out");

    // 6. 再オープンして、各値が完全に復元されていることを検証
    let reopened = ibis_pptx::open(saved).expect("reopen trimmed & faded media pptx");
    let rep_slide = &reopened.presentation.slides[0];
    let rep_video = rep_slide.shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| !c.audio))).unwrap();
    let rep_audio = rep_slide.shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| c.audio))).unwrap();

    if let ShapeKind::Picture(pic) = &rep_video.kind {
        let opt = &pic.clip.as_ref().unwrap().options;
        assert_eq!(opt.trim_start_ms, 1500);
        assert_eq!(opt.trim_end_ms, 6500);
        assert_eq!(opt.fade_in_ms, 1000);
        assert_eq!(opt.fade_out_ms, 2000);
        assert!(opt.is_trimmed());
        assert!(opt.is_faded());
    } else {
        panic!("expected video Picture");
    }

    if let ShapeKind::Picture(pic) = &rep_audio.kind {
        let opt = &pic.clip.as_ref().unwrap().options;
        assert_eq!(opt.trim_start_ms, 2000);
        assert_eq!(opt.trim_end_ms, 8000);
        assert_eq!(opt.fade_in_ms, 500);
        assert_eq!(opt.fade_out_ms, 1500);
        assert!(opt.is_trimmed());
        assert!(opt.is_faded());
    } else {
        panic!("expected audio Picture");
    }

    // 7. Undo で元のトリムなし・フェードなし（0）に戻ることを検証
    assert!(s.document.undo());
    if let ShapeKind::Picture(pic) = &s.p().slides[0].shapes[0].kind {
        let opt = &pic.clip.as_ref().unwrap().options;
        assert_eq!(opt.trim_start_ms, 0);
        assert_eq!(opt.trim_end_ms, 0);
        assert_eq!(opt.fade_in_ms, 0);
        assert_eq!(opt.fade_out_ms, 0);
        assert!(!opt.is_trimmed());
        assert!(!opt.is_faded());
    }

    // 8. Redo で再度トリム・フェードが適用されることを検証
    assert!(s.document.redo());
    if let ShapeKind::Picture(pic) = &s.p().slides[0].shapes[0].kind {
        let opt = &pic.clip.as_ref().unwrap().options;
        assert_eq!(opt.trim_start_ms, 1500);
        assert_eq!(opt.trim_end_ms, 6500);
        assert_eq!(opt.fade_in_ms, 1000);
        assert_eq!(opt.fade_out_ms, 2000);
    }

    // 9. トリム解除（0, 0）とフェード解除（0, 0）で保存し、OOXMLからノードが削除されることを検証
    let mut cleared_video = s.p().slides[0].shapes[0].clone();
    if let ShapeKind::Picture(ref mut pic) = cleared_video.kind {
        if let Some(ref mut c) = pic.clip {
            c.options.trim_start_ms = 0;
            c.options.trim_end_ms = 0;
            c.options.fade_in_ms = 0;
            c.options.fade_out_ms = 0;
        }
    }
    let cleared_saved = s.run(vec![
        Command::SetKind { slide, shape: video_shape_id, kind: Box::new(cleared_video.kind) },
    ]);
    let reopened_cleared = ibis_pptx::open(cleared_saved).expect("reopen cleared media pptx");
    let rep_cleared_video = reopened_cleared.presentation.slides[0].shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| !c.audio))).unwrap();
    if let ShapeKind::Picture(pic) = &rep_cleared_video.kind {
        let opt = &pic.clip.as_ref().unwrap().options;
        assert_eq!(opt.trim_start_ms, 0);
        assert_eq!(opt.trim_end_ms, 0);
        assert_eq!(opt.fade_in_ms, 0);
        assert_eq!(opt.fade_out_ms, 0);
    }
}

/// 動画・音声の再生ブックマーク（p14:bmkLst, p14:bmk: PPTX-VIDEO-014, PPTX-AUDIO-011）の
/// 追加、OOXML差分保存、再オープン、OOXML要素検証、削除、Undo/Redo、全消去を検証。
#[test]
fn test_media_bookmarks_round_trip() {
    use ibis_doc_model::{Clip, ClipOptions, MediaBookmark, MediaStart, MediaVolume};

    let mut s = Scenario::open("media-bookmarks", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;

    // 1. メディア（動画・音声・ポスター画像）を登録
    let video_media = s.document.ids().add_media(MediaFormat::Mp4, vec![0x00, 0x00, 0x00, 0x18, 0x66, 0x74, 0x79, 0x70]);
    let audio_media = s.document.ids().add_media(MediaFormat::M4a, vec![0x00, 0x00, 0x00, 0x20, 0x66, 0x74, 0x79, 0x70]);
    let poster_media = s.document.ids().add_media(MediaFormat::Png, vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

    // 2. 初期クリップ（ブックマークなし）
    let video_clip = Clip {
        media: video_media,
        audio: false,
        options: ClipOptions {
            start: MediaStart::Automatically,
            volume: MediaVolume::High,
            ..Default::default()
        },
        link: None,
    };
    let video_shape_id = s.document.ids().new_shape_id();
    let video_shape = Shape::new(
        video_shape_id,
        EmuRect::new(100_000, 100_000, 3_000_000, 2_000_000),
        ShapeKind::Picture(Picture {
            media: poster_media,
            crop: Default::default(),
            stroke: None,
            corner: 0,
            duotone: None,
            clear: None,
            luminance: None,
            clip: Some(video_clip),
            shadow: None,
            reflection: None,
            soft_edge: None,
            alt_text: Default::default(),
            decorative: false,
            name: "Video 1".to_string(), geometry: None, effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None }),
    );

    let audio_clip = Clip {
        media: audio_media,
        audio: true,
        options: ClipOptions {
            start: MediaStart::OnClick,
            volume: MediaVolume::Medium,
            ..Default::default()
        },
        link: None,
    };
    let audio_shape_id = s.document.ids().new_shape_id();
    let audio_shape = Shape::new(
        audio_shape_id,
        EmuRect::new(4_000_000, 100_000, 914_400, 914_400),
        ShapeKind::Picture(Picture {
            media: poster_media,
            crop: Default::default(),
            stroke: None,
            corner: 0,
            duotone: None,
            clear: None,
            luminance: None,
            clip: Some(audio_clip),
            shadow: None,
            reflection: None,
            soft_edge: None,
            alt_text: Default::default(),
            decorative: false,
            name: "Audio 1".to_string(), geometry: None, effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None }),
    );

    s.run(vec![
        Command::InsertShape { slide, index: 0, shape: video_shape },
        Command::InsertShape { slide, index: 1, shape: audio_shape },
    ]);

    // 3. 動画にブックマーク2件（Bookmark 1: 1500ms, Bookmark 2: 4500ms）を追加
    let mut updated_video = s.p().slides[0].shapes[0].clone();
    if let ShapeKind::Picture(ref mut pic) = updated_video.kind {
        if let Some(ref mut c) = pic.clip {
            c.options.add_bookmark(1500, Some("Bookmark 1".to_string()));
            c.options.add_bookmark(4500, Some("Bookmark 2".to_string()));
        }
    }

    // 4. 音声にブックマーク2件（Intro: 500ms, Chorus: 3000ms）を追加
    let mut updated_audio = s.p().slides[0].shapes[1].clone();
    if let ShapeKind::Picture(ref mut pic) = updated_audio.kind {
        if let Some(ref mut c) = pic.clip {
            c.options.add_bookmark(500, Some("Intro".to_string()));
            c.options.add_bookmark(3000, Some("Chorus".to_string()));
        }
    }

    let saved = s.run(vec![
        Command::SetKind { slide, shape: video_shape_id, kind: Box::new(updated_video.kind.clone()) },
        Command::SetKind { slide, shape: audio_shape_id, kind: Box::new(updated_audio.kind.clone()) },
    ]);

    // 5. OOXML パッケージレベルで p14:bmkLst および p14:bmk が保存されていることを検証
    let package = ibis_ooxml::Package::open(saved.clone()).expect("valid ooxml package");
    let slide_bytes = package.read("ppt/slides/slide1.xml").expect("slide1.xml found");
    let slide_xml = String::from_utf8(slide_bytes).expect("valid utf8 xml");
    assert!(slide_xml.contains("p14:bmkLst"), "contains p14:bmkLst");
    assert!(slide_xml.contains("p14:bmk"), "contains p14:bmk");
    assert!(slide_xml.contains("name=\"Bookmark 1\""), "video bookmark 1 name");
    assert!(slide_xml.contains("time=\"1500\""), "video bookmark 1 time");
    assert!(slide_xml.contains("name=\"Bookmark 2\""), "video bookmark 2 name");
    assert!(slide_xml.contains("time=\"4500\""), "video bookmark 2 time");
    assert!(slide_xml.contains("name=\"Intro\""), "audio bookmark intro name");
    assert!(slide_xml.contains("time=\"500\""), "audio bookmark intro time");
    assert!(slide_xml.contains("name=\"Chorus\""), "audio bookmark chorus name");
    assert!(slide_xml.contains("time=\"3000\""), "audio bookmark chorus time");

    // 6. 再オープンして、各値が完全に復元されていることを検証
    let reopened = ibis_pptx::open(saved).expect("reopen bookmarked media pptx");
    let rep_slide = &reopened.presentation.slides[0];
    let rep_video = rep_slide.shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| !c.audio))).unwrap();
    let rep_audio = rep_slide.shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| c.audio))).unwrap();

    if let ShapeKind::Picture(pic) = &rep_video.kind {
        let opt = &pic.clip.as_ref().unwrap().options;
        assert_eq!(opt.bookmarks.len(), 2);
        assert_eq!(opt.bookmarks[0], MediaBookmark::new("Bookmark 1", 1500));
        assert_eq!(opt.bookmarks[1], MediaBookmark::new("Bookmark 2", 4500));
    } else {
        panic!("expected video Picture");
    }

    if let ShapeKind::Picture(pic) = &rep_audio.kind {
        let opt = &pic.clip.as_ref().unwrap().options;
        assert_eq!(opt.bookmarks.len(), 2);
        assert_eq!(opt.bookmarks[0], MediaBookmark::new("Intro", 500));
        assert_eq!(opt.bookmarks[1], MediaBookmark::new("Chorus", 3000));
    } else {
        panic!("expected audio Picture");
    }

    // 7. Undo で元のブックマークなし（空）に戻ることを検証
    assert!(s.document.undo());
    if let ShapeKind::Picture(pic) = &s.p().slides[0].shapes[0].kind {
        let opt = &pic.clip.as_ref().unwrap().options;
        assert!(opt.bookmarks.is_empty());
    }

    // 8. Redo で再度ブックマークが適用されることを検証
    assert!(s.document.redo());
    if let ShapeKind::Picture(pic) = &s.p().slides[0].shapes[0].kind {
        let opt = &pic.clip.as_ref().unwrap().options;
        assert_eq!(opt.bookmarks.len(), 2);
        assert_eq!(opt.bookmarks[0].time_ms, 1500);
        assert_eq!(opt.bookmarks[1].time_ms, 4500);
    }

    // 9. ブックマーク全消去で保存し、OOXMLからノードが削除されることを検証
    let mut cleared_video = s.p().slides[0].shapes[0].clone();
    if let ShapeKind::Picture(ref mut pic) = cleared_video.kind {
        if let Some(ref mut c) = pic.clip {
            c.options.clear_bookmarks();
        }
    }
    let cleared_saved = s.run(vec![
        Command::SetKind { slide, shape: video_shape_id, kind: Box::new(cleared_video.kind) },
    ]);
    let reopened_cleared = ibis_pptx::open(cleared_saved.clone()).expect("reopen cleared media pptx");
    let rep_cleared_video = reopened_cleared.presentation.slides[0].shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| !c.audio))).unwrap();
    if let ShapeKind::Picture(pic) = &rep_cleared_video.kind {
        let opt = &pic.clip.as_ref().unwrap().options;
        assert!(opt.bookmarks.is_empty());
    }
    let cleared_pkg = ibis_ooxml::Package::open(cleared_saved).expect("valid cleared package");
    let cleared_xml = String::from_utf8(cleared_pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    // 音声にはまだ Intro/Chorus が残っているが、動画のブックマークは削除されている
    assert!(cleared_xml.contains("Intro"));
    assert!(!cleared_xml.contains("Bookmark 1"));
    assert!(!cleared_xml.contains("Bookmark 2"));
}

#[test]
fn test_media_captions_round_trip() {
    use ibis_doc_model::{Clip, ClipOptions};
    let mut s = Scenario::open("media-captions", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;

    let poster_bytes = png(16, 9, [0x30, 0x30, 0x30]);
    let poster_media = s.document.ids().add_media(MediaFormat::Png, poster_bytes);
    let video_bytes = b"fake-video-bytes".to_vec();
    let video_media = s.document.ids().add_media(MediaFormat::Mp4, video_bytes);

    let mut video_clip = Clip {
        media: video_media,
        audio: false,
        options: ClipOptions::default(),
        link: None,
    };
    video_clip.options.add_caption("日本語", "ja-JP", "WEBVTT\n\n00:00:00.000 --> 00:00:05.000\nこんにちは");
    video_clip.options.add_caption("English", "en-US", "WEBVTT\n\n00:00:00.000 --> 00:00:05.000\nHello");

    let video_shape_id = s.document.ids().new_shape_id();
    let video_shape = Shape::new(
        video_shape_id,
        EmuRect::new(0, 0, 1600000, 900000),
        ShapeKind::Picture(Picture {
            media: poster_media,
            crop: Default::default(),
            stroke: None,
            corner: 0,
            duotone: None,
            clear: None,
            luminance: None,
            clip: Some(video_clip),
            shadow: None,
            reflection: None,
            soft_edge: None,
            alt_text: Default::default(),
            decorative: false,
            name: "Video 1".to_string(), geometry: None, effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None }),
    );

    let saved = s.run(vec![
        Command::InsertShape { slide, shape: video_shape, index: 0 },
    ]);

    // 1. 再オープンして、字幕トラックが正しく保持されていることを検証
    let reopened = ibis_pptx::open(saved.clone()).expect("reopen media captions pptx");
    let rep_slide = &reopened.presentation.slides[0];
    let rep_video = rep_slide.shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.is_some())).unwrap();
    if let ShapeKind::Picture(pic) = &rep_video.kind {
        let opt = &pic.clip.as_ref().unwrap().options;
        assert_eq!(opt.captions.len(), 2);
        assert_eq!(opt.captions[0].label, "日本語");
        assert_eq!(opt.captions[0].lang, "ja-JP");
        assert!(opt.captions[0].vtt.contains("こんにちは"));
        assert_eq!(opt.captions[1].label, "English");
        assert_eq!(opt.captions[1].lang, "en-US");
        assert!(opt.captions[1].vtt.contains("Hello"));
    } else {
        panic!("expected Picture");
    }

    // 2. OOXML パッケージに captionTrack が出力されていることを検証
    let pkg = ibis_ooxml::Package::open(saved).expect("open package");
    let xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(xml.contains("captionTrack"));
    assert!(xml.contains("日本語"));
    assert!(xml.contains("English"));

    // 3. 字幕の全消去を行って保存し、正しく消去されることを検証
    let mut cleared_video = s.p().slides[0].shapes[0].clone();
    if let ShapeKind::Picture(ref mut pic) = cleared_video.kind {
        if let Some(ref mut c) = pic.clip {
            c.options.clear_captions();
        }
    }
    let cleared_saved = s.run(vec![
        Command::SetKind { slide, shape: video_shape_id, kind: Box::new(cleared_video.kind) },
    ]);
    let reopened_cleared = ibis_pptx::open(cleared_saved).expect("reopen cleared pptx");
    let rep_cleared_video = &reopened_cleared.presentation.slides[0].shapes[0];
    if let ShapeKind::Picture(pic) = &rep_cleared_video.kind {
        let opt = &pic.clip.as_ref().unwrap().options;
        assert!(opt.captions.is_empty());
    }
}

#[test]
fn test_video_border_shape_and_effects_round_trip() {
    use ibis_doc_model::{Clip, ClipOptions, Reflection, Shadow, TileAlign};
    let mut s = Scenario::open("video-border-shape-effects", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;

    let poster_bytes = png(16, 9, [0x20, 0x20, 0x20]);
    let poster_media = s.document.ids().add_media(MediaFormat::Png, poster_bytes);
    let video_bytes = b"fake-mp4-data".to_vec();
    let video_media = s.document.ids().add_media(MediaFormat::Mp4, video_bytes);

    let video_clip = Clip {
        media: video_media,
        audio: false,
        options: ClipOptions::default(),
        link: None,
    };

    let video_shape_id = s.document.ids().new_shape_id();
    let stroke = Stroke::solid(Color::opaque(Rgb(0x0070C0)), 25_400); // 2pt blue
    let shadow = Shadow {
        color: Color::opaque(Rgb(0x404040)),
        blur: 50_000,
        distance: 38_100,
        direction: 5_400_000,
        sx: 100_000,
        sy: 100_000,
        kx: 0,
        ky: 0,
        align: TileAlign::Bottom,
        rot_with_shape: true,
    };
    let reflection = Reflection {
        blur: 10_000,
        start_alpha: 50_000,
        end_alpha: 0,
        start_pos: 0,
        end_pos: 100_000,
        distance: 20_000,
        direction: 5_400_000,
        fade_direction: 5_400_000,
        sx: 100_000,
        sy: 100_000,
        kx: 0,
        ky: 0,
        align: TileAlign::Bottom,
        rot_with_shape: true,
    };
    let soft_edge = 63_500; // 5pt

    let video_shape = Shape::new(
        video_shape_id,
        EmuRect::new(100_000, 100_000, 3_000_000, 2_000_000),
        ShapeKind::Picture(Picture {
            media: poster_media,
            crop: Default::default(),
            stroke: Some(stroke.clone()),
            corner: 20_000, // 20% 角丸
            duotone: None,
            clear: None,
            luminance: None,
            clip: Some(video_clip),
            shadow: Some(shadow.clone()),
            reflection: Some(reflection.clone()),
            soft_edge: Some(soft_edge),
            alt_text: Default::default(),
            decorative: false,
            name: "Styled Video".to_string(), geometry: None, effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None }),
    );

    let saved_bytes = s.run(vec![
        Command::InsertShape { slide, index: 0, shape: video_shape },
    ]);

    // 1. OOXML の検証
    let pkg = ibis_ooxml::Package::open(saved_bytes.clone()).expect("valid package");
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();

    // 枠線 ln
    assert!(slide_xml.contains(r#"w="25400""#), "XML should contain border stroke width 25400");
    // 図形 角丸 roundRect
    assert!(slide_xml.contains(r#"prst="roundRect""#), "XML should contain roundRect");
    assert!(slide_xml.contains(r#"fmla="val 20000""#), "XML should contain corner radius 20000");
    // 効果 effectLst
    assert!(slide_xml.contains("<a:effectLst>"), "XML should contain effectLst");
    assert!(slide_xml.contains(r#"<a:outerShdw blurRad="50000" dist="38100" dir="5400000""#), "XML should contain outerShdw");
    assert!(slide_xml.contains(r#"<a:reflection blurRad="10000""#), "XML should contain reflection");
    assert!(slide_xml.contains(r#"<a:softEdge rad="63500"/>"#), "XML should contain softEdge");

    // 2. 再オープンしてモデルの復元検証
    let reopened = ibis_pptx::open(saved_bytes).expect("reopen pptx");
    let rep_video = reopened.presentation.slides[0].shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| !c.audio))).unwrap();
    if let ShapeKind::Picture(pic) = &rep_video.kind {
        assert_eq!(pic.stroke.as_ref().unwrap().width, 25_400);
        assert_eq!(pic.corner, 20_000);
        assert_eq!(pic.shadow.as_ref().unwrap().blur, 50_000);
        assert_eq!(pic.shadow.as_ref().unwrap().distance, 38_100);
        assert_eq!(pic.reflection.as_ref().unwrap().blur, 10_000);
        assert_eq!(pic.soft_edge, Some(63_500));
    } else {
        panic!("expected Picture");
    }

    // 3. 効果を解除（四角形に戻し、枠線・影・反射・ぼかしクリア）して差分保存
    let mut updated_shape = s.p().slides[0].shapes[0].clone();
    if let ShapeKind::Picture(ref mut pic) = updated_shape.kind {
        pic.stroke = None;
        pic.corner = 0;
        pic.shadow = None;
        pic.reflection = None;
        pic.soft_edge = None;
    }
    let cleared_saved = s.run(vec![
        Command::SetKind { slide, shape: video_shape_id, kind: Box::new(updated_shape.kind) },
    ]);

    let reopened_cleared = ibis_pptx::open(cleared_saved.clone()).expect("reopen cleared pptx");
    let rep_cleared = reopened_cleared.presentation.slides[0].shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.clip.as_ref().is_some_and(|c| !c.audio))).unwrap();
    if let ShapeKind::Picture(pic) = &rep_cleared.kind {
        assert!(pic.stroke.is_none());
        assert_eq!(pic.corner, 0);
        assert!(pic.shadow.is_none());
        assert!(pic.reflection.is_none());
        assert!(pic.soft_edge.is_none());
    }

    // 4. OOXMLからエフェクトが綺麗に削除されていることを検証
    let cleared_pkg = ibis_ooxml::Package::open(cleared_saved).expect("valid cleared package");
    let cleared_xml = String::from_utf8(cleared_pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(!cleared_xml.contains("outerShdw"));
    assert!(!cleared_xml.contains("softEdge"));

    // 5. Undo で再復元されることを検証
    assert!(s.document.undo());
    if let ShapeKind::Picture(pic) = &s.p().slides[0].shapes[0].kind {
        assert_eq!(pic.corner, 20_000);
        assert!(pic.stroke.is_some());
        assert!(pic.shadow.is_some());
        assert!(pic.reflection.is_some());
        assert_eq!(pic.soft_edge, Some(63_500));
    }
}

/// PPTX-TEXT-002: WordArt 文字変形 (`a:prstTxWarp`) の読み書き・差分保存・新規保存・ラウンドトリップテスト。
#[test]
fn test_wordart_warp_and_styles_round_trip() {
    let mut s = Scenario::open("wordart-warp", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let shape = s.p().slides[0].shapes.iter().find(|sh| sh.text_body().is_some()).unwrap().id;

    // 1. 変形を textArchUp に設定して保存
    let mut body1 = s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().clone();
    assert_eq!(body1.warp, None);
    body1.warp = Some("textArchUp".to_string());
    let saved1 = s.run(vec![Command::SetTextBody { slide, shape, body: body1 }]);

    // 再読込して warp が保持されているか確認
    let reopened1 = ibis_pptx::open(saved1.clone()).expect("reopen pptx with textArchUp");
    let re_shape1 = reopened1.presentation.slide(slide).unwrap().shape(shape).unwrap();
    assert_eq!(re_shape1.text_body().unwrap().warp.as_deref(), Some("textArchUp"));

    // XML に a:prstTxWarp prst="textArchUp" が含まれているか確認
    let pkg1 = ibis_ooxml::Package::open(saved1).expect("package 1");
    let xml1 = String::from_utf8(pkg1.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(xml1.contains("prst=\"textArchUp\""));

    // 2. 変形を textWave1 に変更して保存
    let mut body2 = s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().clone();
    body2.warp = Some("textWave1".to_string());
    let saved2 = s.run(vec![Command::SetTextBody { slide, shape, body: body2 }]);

    let reopened2 = ibis_pptx::open(saved2.clone()).expect("reopen pptx with textWave1");
    let re_shape2 = reopened2.presentation.slide(slide).unwrap().shape(shape).unwrap();
    assert_eq!(re_shape2.text_body().unwrap().warp.as_deref(), Some("textWave1"));

    let pkg2 = ibis_ooxml::Package::open(saved2).expect("package 2");
    let xml2 = String::from_utf8(pkg2.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(xml2.contains("prst=\"textWave1\""));
    assert!(!xml2.contains("prst=\"textArchUp\""));

    // 3. 変形を解除 (None) して保存
    let mut body3 = s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().clone();
    body3.warp = None;
    let saved3 = s.run(vec![Command::SetTextBody { slide, shape, body: body3 }]);

    let reopened3 = ibis_pptx::open(saved3.clone()).expect("reopen pptx without warp");
    let re_shape3 = reopened3.presentation.slide(slide).unwrap().shape(shape).unwrap();
    assert_eq!(re_shape3.text_body().unwrap().warp, None);

    let pkg3 = ibis_ooxml::Package::open(saved3).expect("package 3");
    let xml3 = String::from_utf8(pkg3.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(!xml3.contains("prstTxWarp"));

    // 4. Undo 検証
    assert!(s.document.undo());
    assert_eq!(s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().warp.as_deref(), Some("textWave1"));
    assert!(s.document.undo());
    assert_eq!(s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().warp.as_deref(), Some("textArchUp"));
    assert!(s.document.undo());
    assert_eq!(s.p().slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap().warp, None);

    // 5. 新規図形（テキストボックスに warp を設定したもの）の挿入と保存・読込ラウンドトリップ
    let new_shape_id = s.document.ids().new_shape_id();
    let mut new_shape = ibis_doc_model::Shape::text_box(
        new_shape_id,
        ibis_doc_model::EmuRect::new(100_000, 100_000, 3_000_000, 1_000_000),
        "WordArt New Shape Test",
    );
    if let ibis_doc_model::ShapeKind::TextBox(ref mut b) = new_shape.kind {
        b.warp = Some("textCircle".to_string());
    }
    let insert_index = s.p().slides[0].shapes.len();
    let saved_new = s.run(vec![Command::InsertShape { slide, index: insert_index, shape: new_shape }]);

    let reopened_new = ibis_pptx::open(saved_new.clone()).expect("reopen pptx with new textCircle shape");
    let re_new_shape = &reopened_new.presentation.slides[0].shapes[insert_index];
    assert_eq!(re_new_shape.text_body().unwrap().warp.as_deref(), Some("textCircle"));

    let pkg_new = ibis_ooxml::Package::open(saved_new).expect("pkg new");
    let xml_new = String::from_utf8(pkg_new.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(xml_new.contains("prst=\"textCircle\""));
}

#[test]
fn test_convert_svg_to_shape_and_cust_geom_round_trip() {
    let mut s = Scenario::open("svg-convert", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;

    // 1. SVGベクター画像データ
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="150" viewBox="0 0 200 150">
        <rect x="20" y="20" width="160" height="110" rx="10" ry="10" fill="#2e75b6"/>
        <circle cx="100" cy="75" r="30" fill="#ffc000"/>
    </svg>"##;

    let frame = ibis_doc_model::EmuRect::new(1_000_000, 1_000_000, 3_000_000, 2_250_000);

    // 2. convert_svg_to_shapes による変換
    let converted = ibis_pptx::convert_svg_to_shapes(|| s.document.ids().new_shape_id(), svg.as_bytes(), frame)
        .expect("convert svg to shapes succeeds");
    assert_eq!(converted.len(), 2, "SVG has 2 vector elements (rect and circle)");

    // 各図形が Geometry::Formula (preset: None) であることを確認
    for shape in &converted {
        let ibis_doc_model::ShapeKind::Auto(auto) = &shape.kind else { panic!("expected AutoShape") };
        assert!(matches!(&auto.geometry, ibis_doc_model::Geometry::Formula(f) if f.preset.is_none()));
    }

    // 3. スライドへ挿入して差分保存
    let mut commands = Vec::new();
    let base_index = s.p().slides[0].shapes.len();
    for (i, shape) in converted.into_iter().enumerate() {
        commands.push(Command::InsertShape {
            slide,
            index: base_index + i,
            shape,
        });
    }

    let saved = s.run(commands);

    // 4. OOXML (slide1.xml) の <a:custGeom> 要素と子要素 sequence を検証
    let pkg = ibis_ooxml::Package::open(saved.clone()).expect("pkg open");
    let xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(xml.contains("<a:custGeom>"), "OOXML must contain <a:custGeom>");
    assert!(xml.contains("<a:pathLst>"), "OOXML must contain <a:pathLst>");
    assert!(xml.contains("<a:moveTo>"), "OOXML must contain <a:moveTo>");
    assert!(xml.contains("<a:cubicBezTo>"), "OOXML must contain <a:cubicBezTo>");
    assert!(xml.contains("<a:close/>"), "OOXML must contain <a:close/>");

    // 5. 再読込（ラウンドトリップ検証）
    let reopened = ibis_pptx::open(saved).expect("reopen pptx with converted custom shapes");
    let shapes = &reopened.presentation.slides[0].shapes;
    assert!(shapes.len() >= base_index + 2);

    let re_shape1 = &shapes[base_index];
    let re_shape2 = &shapes[base_index + 1];

    let ibis_doc_model::ShapeKind::Auto(re_auto1) = &re_shape1.kind else { panic!("shape 1 must be AutoShape") };
    let ibis_doc_model::ShapeKind::Auto(re_auto2) = &re_shape2.kind else { panic!("shape 2 must be AutoShape") };

    assert!(matches!(&re_auto1.geometry, ibis_doc_model::Geometry::Formula(f) if f.preset.is_none()));
    assert!(matches!(&re_auto2.geometry, ibis_doc_model::Geometry::Formula(f) if f.preset.is_none()));

    assert_eq!(re_auto1.fill, ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x2e75b6))));
    assert_eq!(re_auto2.fill, ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffc000))));
}

#[test]
fn test_freehand_shape_save_and_round_trip() {
    let mut s = Scenario::open("freehand-shape", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let shape_id = s.document.ids().new_shape_id();
    let frame = EmuRect::new(1_000_000, 1_500_000, 2_000_000, 1_200_000);

    // 1. フリーハンド図形（FormulaGeometry: preset=None, ops=[MoveTo, LineTo...]）を作成
    let path = ibis_doc_model::GeometryPath {
        width: frame.width,
        height: frame.height,
        fill: ibis_doc_model::PathFill::None,
        stroke: true,
        ops: vec![
            ibis_doc_model::PathOp::MoveTo("0".into(), "0".into()),
            ibis_doc_model::PathOp::LineTo("500000".into(), "800000".into()),
            ibis_doc_model::PathOp::LineTo("1200000".into(), "400000".into()),
            ibis_doc_model::PathOp::LineTo("2000000".into(), "1200000".into()),
        ],
    };
    let def = ibis_doc_model::GeometryDef {
        adjust: Vec::new(),
        guides: Vec::new(),
        paths: vec![path],
        text_rect: None,
    };
    let geom = ibis_doc_model::FormulaGeometry {
        preset: None,
        adjust: Vec::new(),
        def: std::sync::Arc::new(def),
    };
    let stroke = ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x4472c4)), 25_400); // 2pt
    let auto = ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Formula(geom), ibis_doc_model::Fill::None, Some(stroke));
    let shape = Shape::new(shape_id, frame, ShapeKind::Auto(auto));

    // 2. スライドへ挿入
    let base_index = s.p().slides[0].shapes.len();
    let saved = s.run(vec![Command::InsertShape {
        slide,
        index: base_index,
        shape,
    }]);

    // 3. OOXML (slide1.xml) の <a:custGeom> 要素と子要素 sequence を検証
    let pkg = ibis_ooxml::Package::open(saved.clone()).expect("pkg open");
    let xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(xml.contains("<a:custGeom>"), "OOXML must contain <a:custGeom>");
    assert!(xml.contains("<a:pathLst>"), "OOXML must contain <a:pathLst>");
    assert!(xml.contains("<a:moveTo>"), "OOXML must contain <a:moveTo>");
    assert!(xml.contains("<a:lnTo>"), "OOXML must contain <a:lnTo>");
    assert!(xml.contains("w=\"2000000\""), "path width must match frame width");
    assert!(xml.contains("h=\"1200000\""), "path height must match frame height");

    // 4. 再読込（ラウンドトリップ検証）
    let reopened = ibis_pptx::open(saved).expect("reopen pptx with freehand shape");
    let shapes = &reopened.presentation.slides[0].shapes;
    assert_eq!(shapes.len(), base_index + 1);

    let re_shape = &shapes[base_index];
    assert_eq!(re_shape.frame, frame);
    let ShapeKind::Auto(re_auto) = &re_shape.kind else { panic!("freehand shape must be AutoShape") };
    assert!(matches!(&re_auto.geometry, ibis_doc_model::Geometry::Formula(f) if f.preset.is_none()));

    if let ibis_doc_model::Geometry::Formula(formula) = &re_auto.geometry {
        assert_eq!(formula.def.paths.len(), 1);
        let p = &formula.def.paths[0];
        assert_eq!(p.width, frame.width);
        assert_eq!(p.height, frame.height);
        assert_eq!(p.ops.len(), 4);
        assert!(matches!(&p.ops[0], ibis_doc_model::PathOp::MoveTo(x, y) if x == "0" && y == "0"));
        assert!(matches!(&p.ops[1], ibis_doc_model::PathOp::LineTo(x, y) if x == "500000" && y == "800000"));
        assert!(matches!(&p.ops[2], ibis_doc_model::PathOp::LineTo(x, y) if x == "1200000" && y == "400000"));
        assert!(matches!(&p.ops[3], ibis_doc_model::PathOp::LineTo(x, y) if x == "2000000" && y == "1200000"));
    }

    assert_eq!(re_auto.stroke.as_ref().map(|s| s.width), Some(25_400));
    assert_eq!(re_auto.stroke.as_ref().map(|s| s.color), Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x4472c4))));

    // 5. Undo と Redo
    s.document.undo();
    assert_eq!(s.p().slides[0].shapes.len(), base_index);
    s.document.redo();
    assert_eq!(s.p().slides[0].shapes.len(), base_index + 1);
}

#[test]
fn test_insert_icon_save_and_round_trip() {
    let mut s = Scenario::open("insert-icon", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let base_index = s.p().slides[0].shapes.len();

    // 1. アイコンの SVG ベクターデータ（例: Star アイコン）を準備
    let svg_data = b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><path d=\"M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z\"/></svg>".to_vec();
    let format = ibis_doc_model::MediaFormat::Svg;
    let media = s.document.ids().add_media(format, svg_data.clone());

    // 2. アイコン Picture 図形を構築（1.5 インチ四方 = 1,371,600 EMU）
    let frame = EmuRect::new(2_000_000, 1_500_000, 1_371_600, 1_371_600);
    let id = s.document.ids().new_shape_id();
    let name = "Icon: star".to_string();
    let picture = ibis_doc_model::Picture {
        media,
        crop: Default::default(),
        stroke: None,
        corner: 0,
        duotone: None,
        clear: None,
        luminance: None,
        clip: None,
        alt_text: Default::default(),
        decorative: false,
        name: name.clone(),
        shadow: None,
        reflection: None,
        soft_edge: None, geometry: None, effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None };
    let shape = Shape::new(id, frame, ShapeKind::Picture(picture));

    // 3. スライドへ挿入・保存
    let saved = s.run(vec![
        Command::InsertShape {
            slide,
            index: base_index,
            shape,
        },
        Command::SetAspectLocked {
            slide,
            shape: id,
            locked: true,
        },
    ]);

    // 4. OOXML (slide1.xml) の <p:pic> および SVG 拡張 Blip を検証
    let pkg = ibis_ooxml::Package::open(saved.clone()).expect("pkg open");
    let xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(xml.contains("<p:pic>"), "must contain <p:pic>");
    assert!(xml.contains("Icon: star"), "must contain icon shape name");
    assert!(xml.contains("asvg:svgBlip"), "must contain SVG Blip extension");

    // 5. メディアパートの永続化検証
    let content_types = String::from_utf8(pkg.read("[Content_Types].xml").unwrap()).unwrap();
    assert!(content_types.contains("image/svg+xml"), "Content_Types must register SVG");

    // 6. 再読込（ラウンドトリップ検証）
    let reopened = ibis_pptx::open(saved).expect("reopen pptx with inserted icon");
    let shapes = &reopened.presentation.slides[0].shapes;
    assert_eq!(shapes.len(), base_index + 1);

    let re_shape = &shapes[base_index];
    assert_eq!(re_shape.frame, frame);
    let ShapeKind::Picture(re_pic) = &re_shape.kind else { panic!("must be Picture") };
    assert_eq!(re_pic.name, "Icon: star");

    let media_info = reopened.presentation.media.iter().find(|m| m.id == re_pic.media).expect("media exists");
    assert_eq!(media_info.format, ibis_doc_model::MediaFormat::Svg);

    // 7. Undo と Redo
    s.document.undo();
    assert_eq!(s.p().slides[0].shapes.len(), base_index);
    s.document.redo();
    assert_eq!(s.p().slides[0].shapes.len(), base_index + 1);
}

/// スライドショー中のインク描画（ペン・蛍光ペン）保持機能（PPTX-INK-012）:
/// スライドショーで描かれた複数インクストローク（ペン・蛍光ペン）から生成されたカスタム手描き図形を挿入し、
/// DrawingML <a:custGeom> 保存、色・線幅・半透明度・座標の再読込保持、Undo/Redo を検証する。
#[test]
fn test_keep_slideshow_ink_shapes_round_trip() {
    let mut s = Scenario::open("keep-ink", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let base_count = s.p().slides[0].shapes.len();

    let make_ink_shape = |id: ShapeId, frame: EmuRect, points: &[(i64, i64)], stroke: ibis_doc_model::Stroke| {
        let ops = points.iter().enumerate().map(|(i, &(x, y))| {
            if i == 0 {
                ibis_doc_model::PathOp::MoveTo(x.to_string(), y.to_string())
            } else {
                ibis_doc_model::PathOp::LineTo(x.to_string(), y.to_string())
            }
        }).collect();
        let path = ibis_doc_model::GeometryPath {
            width: frame.width,
            height: frame.height,
            fill: ibis_doc_model::PathFill::None,
            stroke: true,
            ops,
        };
        let def = ibis_doc_model::GeometryDef {
            adjust: Vec::new(),
            guides: Vec::new(),
            paths: vec![path],
            text_rect: None,
        };
        let geom = ibis_doc_model::FormulaGeometry {
            preset: None,
            adjust: Vec::new(),
            def: std::sync::Arc::new(def),
        };
        let auto = ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Formula(geom), ibis_doc_model::Fill::None, Some(stroke));
        Shape::new(id, frame, ShapeKind::Auto(auto))
    };

    // 1. ペンインク図形（赤、28,575 EMU）
    let pen_id = s.document.ids().new_shape_id();
    let pen_frame = EmuRect::new(1_000_000, 1_000_000, 2_000_000, 1_500_000);
    let pen_points = vec![(0, 0), (500_000, 800_000), (1_200_000, 400_000), (2_000_000, 1_500_000)];
    let mut pen_stroke = ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xff0000)), 28_575);
    pen_stroke.cap = ibis_doc_model::LineCap::Round;
    pen_stroke.join = ibis_doc_model::LineJoin::Round;
    let pen_shape = make_ink_shape(pen_id, pen_frame, &pen_points, pen_stroke);

    // 2. 蛍光ペンインク図形（黄、半透明 128、152,400 EMU）
    let hl_id = s.document.ids().new_shape_id();
    let hl_frame = EmuRect::new(2_000_000, 3_000_000, 3_000_000, 100_000);
    let hl_points = vec![(0, 50_000), (1_500_000, 50_000), (3_000_000, 50_000)];
    let mut hl_stroke = ibis_doc_model::Stroke::solid(ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0xffff00), alpha: 128 }, 152_400);
    hl_stroke.cap = ibis_doc_model::LineCap::Round;
    hl_stroke.join = ibis_doc_model::LineJoin::Round;
    let hl_shape = make_ink_shape(hl_id, hl_frame, &hl_points, hl_stroke);

    // スライドへ挿入
    let saved = s.run(vec![
        Command::InsertShape { slide, index: base_count, shape: pen_shape },
        Command::InsertShape { slide, index: base_count + 1, shape: hl_shape },
    ]);

    // OOXML XML検証
    let pkg = ibis_ooxml::Package::open(saved.clone()).expect("open package");
    let xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(xml.contains("custGeom"), "must contain custGeom");
    assert!(xml.contains("moveTo"), "must contain moveTo");
    assert!(xml.contains("lnTo"), "must contain lnTo");

    // 再読込ラウンドトリップ検証
    let reopened = ibis_pptx::open(saved).expect("reopen pptx");
    let re_shapes = &reopened.presentation.slides[0].shapes;
    assert_eq!(re_shapes.len(), base_count + 2);

    let re_pen = &re_shapes[base_count];
    assert_eq!(re_pen.frame, pen_frame);
    let ShapeKind::Auto(re_pen_auto) = &re_pen.kind else { panic!("must be AutoShape") };
    assert!(re_pen_auto.stroke.is_some());
    let stroke = re_pen_auto.stroke.as_ref().unwrap();
    assert_eq!(stroke.color.rgb, ibis_doc_model::Rgb(0xff0000));
    assert_eq!(stroke.width, 28_575);

    let re_hl = &re_shapes[base_count + 1];
    assert_eq!(re_hl.frame, hl_frame);
    let ShapeKind::Auto(re_hl_auto) = &re_hl.kind else { panic!("must be AutoShape") };
    assert!(re_hl_auto.stroke.is_some());
    let hl_s = re_hl_auto.stroke.as_ref().unwrap();
    assert_eq!(hl_s.color.rgb, ibis_doc_model::Rgb(0xffff00));
    assert_eq!(hl_s.width, 152_400);

    // Undo / Redo (一括挿入なので1ステップでインク図形群が元に戻る)
    s.document.undo();
    assert_eq!(s.p().slides[0].shapes.len(), base_count);
    s.document.redo();
    assert_eq!(s.p().slides[0].shapes.len(), base_count + 2);
}

/// インク手描きの鉛筆ツール（PPTX-INK-001）:
/// 鉛筆ストローク（細線 0.5pt = 6,350 EMU、半透明グラファイト調 alpha: 192）から生成されたカスタム手描き図形を挿入し、
/// DrawingML <a:custGeom> 保存、色・線幅・半透明度・座標の再読込保持、Undo/Redo を検証する。
#[test]
fn test_pencil_ink_shape_round_trip() {
    let mut s = Scenario::open("pencil-ink", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let base_count = s.p().slides[0].shapes.len();

    let make_ink_shape = |id: ShapeId, frame: EmuRect, points: &[(i64, i64)], stroke: ibis_doc_model::Stroke| {
        let ops = points.iter().enumerate().map(|(i, &(x, y))| {
            if i == 0 {
                ibis_doc_model::PathOp::MoveTo(x.to_string(), y.to_string())
            } else {
                ibis_doc_model::PathOp::LineTo(x.to_string(), y.to_string())
            }
        }).collect();
        let path = ibis_doc_model::GeometryPath {
            width: frame.width,
            height: frame.height,
            fill: ibis_doc_model::PathFill::None,
            stroke: true,
            ops,
        };
        let def = ibis_doc_model::GeometryDef {
            adjust: Vec::new(),
            guides: Vec::new(),
            paths: vec![path],
            text_rect: None,
        };
        let geom = ibis_doc_model::FormulaGeometry {
            preset: None,
            adjust: Vec::new(),
            def: std::sync::Arc::new(def),
        };
        let auto = ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Formula(geom), ibis_doc_model::Fill::None, Some(stroke));
        Shape::new(id, frame, ShapeKind::Auto(auto))
    };

    // 鉛筆インク図形（グラファイトグレー 0x404040、半透明 192、0.5pt = 6,350 EMU）
    let pencil_id = s.document.ids().new_shape_id();
    let pencil_frame = EmuRect::new(1_200_000, 1_200_000, 1_800_000, 1_000_000);
    let pencil_points = vec![(0, 0), (600_000, 500_000), (1_200_000, 300_000), (1_800_000, 1_000_000)];
    let mut pencil_stroke = ibis_doc_model::Stroke::solid(ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0x404040), alpha: 192 }, 6_350);
    pencil_stroke.cap = ibis_doc_model::LineCap::Round;
    pencil_stroke.join = ibis_doc_model::LineJoin::Round;
    let pencil_shape = make_ink_shape(pencil_id, pencil_frame, &pencil_points, pencil_stroke);

    // スライドへ挿入
    let saved = s.run(vec![
        Command::InsertShape { slide, index: base_count, shape: pencil_shape },
    ]);

    // OOXML XML検証
    let pkg = ibis_ooxml::Package::open(saved.clone()).expect("open package");
    let xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(xml.contains("custGeom"), "must contain custGeom");
    assert!(xml.contains("moveTo"), "must contain moveTo");
    assert!(xml.contains("lnTo"), "must contain lnTo");
    assert!(xml.contains("404040"), "must contain pencil graphite color");

    // 再読込ラウンドトリップ検証
    let reopened = ibis_pptx::open(saved).expect("reopen pptx");
    let re_shapes = &reopened.presentation.slides[0].shapes;
    assert_eq!(re_shapes.len(), base_count + 1);

    let re_pencil = &re_shapes[base_count];
    assert_eq!(re_pencil.frame, pencil_frame);
    let ShapeKind::Auto(re_pencil_auto) = &re_pencil.kind else { panic!("must be AutoShape") };
    assert!(re_pencil_auto.stroke.is_some());
    let stroke = re_pencil_auto.stroke.as_ref().unwrap();
    assert_eq!(stroke.color.rgb, ibis_doc_model::Rgb(0x404040));
    assert_eq!(stroke.color.alpha, 192, "pencil stroke must preserve 75% opacity (alpha 192)");
    assert_eq!(stroke.width, 6_350, "pencil stroke width must be 0.5pt (6,350 EMU)");

    // Undo / Redo
    s.document.undo();
    assert_eq!(s.p().slides[0].shapes.len(), base_count);
    s.document.redo();
    assert_eq!(s.p().slides[0].shapes.len(), base_count + 1);
}

/// スライド ズーム機能（PPTX-TRANS-014）:
/// 別のスライドをターゲットとするスライドズーム図形の挿入、
/// OOXML <a:hlinkClick action="ppaction://hlinksldjump"> 出力、
/// 保存・再オープンでの SlideLink::Slide 保持、および Undo/Redo を検証する。
#[test]
fn test_slide_zoom_shape_and_round_trip() {
    let mut s = Scenario::open("slide-zoom", "editing/pptx-plan.pptx");
    assert!(s.p().slides.len() >= 2, "must have at least 2 slides");
    let slide1_id = s.p().slides[0].id;
    let slide2_id = s.p().slides[1].id;
    let base_count = s.p().slides[0].shapes.len();

    use ibis_doc_model::SlideLink;

    // スライドズーム図形の作成
    let zoom_id = s.document.ids().new_shape_id();
    let frame = EmuRect::new(2_000_000, 1_500_000, 2_800_000, 1_575_000);
    let stroke = ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xd0d0d0)), 12_700);
    let fill = ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffffff)));
    let body = ibis_doc_model::TextBody::plain(
        "🔍 Slide 2 (Zoom)\nTarget Summary",
        ibis_doc_model::TextStyle::default(),
    );
    let mut auto = ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect, fill, Some(stroke));
    auto.text = Some(body);
    let shape = Shape::new(zoom_id, frame, ShapeKind::Auto(auto));

    let tooltip = "Slide 2 Zoom: Target Summary".to_string();

    // コマンドを実行して保存
    let saved = s.run(vec![
        Command::InsertShape {
            slide: slide1_id,
            index: base_count,
            shape,
        },
        Command::SetShapeLink {
            slide: slide1_id,
            shape: zoom_id,
            link: Some(SlideLink::Slide(slide2_id)),
        },
        Command::SetShapeLinkTooltip {
            slide: slide1_id,
            shape: zoom_id,
            tooltip: Some(tooltip.clone()),
        },
    ]);

    // OOXML XML検証
    let pkg = ibis_ooxml::Package::open(saved.clone()).expect("open package");
    let xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(xml.contains("hlinkClick"), "must contain hlinkClick for slide zoom");
    assert!(xml.contains("ppaction://hlinksldjump"), "must contain action ppaction://hlinksldjump");

    // 再読込ラウンドトリップ検証
    let reopened = ibis_pptx::open(saved).expect("reopen pptx");
    let re_slide1 = &reopened.presentation.slides[0];
    assert_eq!(re_slide1.shapes.len(), base_count + 1);

    let re_zoom = &re_slide1.shapes[base_count];
    assert_eq!(re_zoom.frame, frame);
    assert_eq!(
        re_slide1.shape_links.get(&re_zoom.id),
        Some(&SlideLink::Slide(slide2_id)),
        "link to target slide preserved"
    );
    assert_eq!(
        re_slide1.shape_link_tooltips.get(&re_zoom.id),
        Some(&tooltip),
        "tooltip preserved"
    );

    // Undo / Redo
    s.document.undo();
    assert_eq!(s.p().slides[0].shapes.len(), base_count);
    assert!(!s.p().slides[0].shape_links.contains_key(&zoom_id));
    s.document.redo();
    assert_eq!(s.p().slides[0].shapes.len(), base_count + 1);
    assert_eq!(
        s.p().slides[0].shape_links.get(&zoom_id),
        Some(&SlideLink::Slide(slide2_id))
    );
}

/// セクション ズーム機能（PPTX-TRANS-015）:
/// 別のセクションの先頭スライドをターゲットとするセクションズーム図形の挿入、
/// OOXML <a:hlinkClick action="ppaction://hlinksldjump"> 出力、
/// 保存・再オープンでの SlideLink::Slide 保持、および Undo/Redo を検証する。
#[test]
fn test_section_zoom_shape_and_round_trip() {
    let mut s = Scenario::open("section-zoom", "editing/pptx-plan.pptx");
    assert!(s.p().slides.len() >= 2, "must have at least 2 slides");
    let slide_ids: Vec<_> = s.p().slides.iter().map(|s| s.id).collect();
    let slide1_id = slide_ids[0];
    let slide2_id = slide_ids[1];
    let base_count = s.p().slides[0].shapes.len();

    use ibis_doc_model::{Section, SlideLink};

    // セクションを作成（全スライドを分割してカバー）
    let sections = vec![
        Section {
            id: "{11111111-1111-1111-1111-111111111111}".into(),
            name: "第1章: 導入".into(),
            slides: vec![slide1_id],
        },
        Section {
            id: "{22222222-2222-2222-2222-222222222222}".into(),
            name: "第2章: 本編".into(),
            slides: slide_ids[1..].to_vec(),
        },
    ];

    // セクションズーム図形の作成（第2章の先頭スライド slide2_id へのジャンプ）
    let zoom_id = s.document.ids().new_shape_id();
    let frame = EmuRect::new(2_000_000, 1_500_000, 2_800_000, 1_575_000);
    let stroke = ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xd0d0d0)), 12_700);
    let fill = ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffffff)));
    let body = ibis_doc_model::TextBody::plain(
        "🔍 Section: 第2章: 本編 (Zoom)\nTarget Summary",
        ibis_doc_model::TextStyle::default(),
    );
    let mut auto = ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect, fill, Some(stroke));
    auto.text = Some(body);
    let shape = Shape::new(zoom_id, frame, ShapeKind::Auto(auto));

    let tooltip = "Section Zoom: 第2章: 本編 (Slide 2)".to_string();

    // コマンドを実行して保存
    let saved = s.run(vec![
        Command::SetSections { sections },
        Command::InsertShape {
            slide: slide1_id,
            index: base_count,
            shape,
        },
        Command::SetShapeLink {
            slide: slide1_id,
            shape: zoom_id,
            link: Some(SlideLink::Slide(slide2_id)),
        },
        Command::SetShapeLinkTooltip {
            slide: slide1_id,
            shape: zoom_id,
            tooltip: Some(tooltip.clone()),
        },
    ]);

    // OOXML XML検証
    let pkg = ibis_ooxml::Package::open(saved.clone()).expect("open package");
    let xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(xml.contains("hlinkClick"), "must contain hlinkClick for section zoom");
    assert!(xml.contains("ppaction://hlinksldjump"), "must contain action ppaction://hlinksldjump");

    // 再読込ラウンドトリップ検証
    let reopened = ibis_pptx::open(saved).expect("reopen pptx");
    assert_eq!(reopened.presentation.sections.len(), 2, "sections preserved");
    let re_slide1 = &reopened.presentation.slides[0];
    assert_eq!(re_slide1.shapes.len(), base_count + 1);

    let re_zoom = &re_slide1.shapes[base_count];
    assert_eq!(re_zoom.frame, frame);
    assert_eq!(
        re_slide1.shape_links.get(&re_zoom.id),
        Some(&SlideLink::Slide(slide2_id)),
        "link to target slide preserved"
    );
    assert_eq!(
        re_slide1.shape_link_tooltips.get(&re_zoom.id),
        Some(&tooltip),
        "tooltip preserved"
    );

    // Undo / Redo
    s.document.undo();
    assert_eq!(s.p().slides[0].shapes.len(), base_count);
    assert!(!s.p().slides[0].shape_links.contains_key(&zoom_id));
    s.document.redo();
    assert_eq!(s.p().slides[0].shapes.len(), base_count + 1);
    assert_eq!(
        s.p().slides[0].shape_links.get(&zoom_id),
        Some(&SlideLink::Slide(slide2_id))
    );
}

/// PPTX-TRANS-016: ズーム後に元へ戻る（Return to Zoom）
/// OOXML <p16:zoom returnToZoom="1"> 出力、解除時の要素削除、
/// 再読込ラウンドトリップ保持、Undo/Redo の検証。
#[test]
fn test_return_to_zoom_shape_and_round_trip() {
    use ibis_doc_model::SlideLink;
    let mut s = Scenario::open("return-to-zoom", "editing/pptx-plan.pptx");
    assert!(s.p().slides.len() >= 2, "must have at least 2 slides");
    let slide1_id = s.p().slides[0].id;
    let slide2_id = s.p().slides[1].id;
    let base_count = s.p().slides[0].shapes.len();

    // ズーム図形の作成（スライド2へのリンク付き）
    let zoom_id = s.document.ids().new_shape_id();
    let frame = EmuRect::new(1_000_000, 1_000_000, 2_000_000, 1_500_000);
    let shape = Shape::new(
        zoom_id,
        frame,
        ShapeKind::Auto(ibis_doc_model::AutoShape::new(
            ibis_doc_model::Geometry::Rect,
            ibis_doc_model::Fill::Solid(ibis_doc_model::Color::WHITE),
            None,
        )),
    );

    // ズーム図形を挿入し、Return to Zoom を有効化
    let saved = s.run(vec![
        Command::InsertShape {
            slide: slide1_id,
            index: base_count,
            shape,
        },
        Command::SetShapeLink {
            slide: slide1_id,
            shape: zoom_id,
            link: Some(SlideLink::Slide(slide2_id)),
        },
        Command::SetShapeReturnToZoom {
            slide: slide1_id,
            shape: zoom_id,
            return_to_zoom: true,
        },
    ]);

    // OOXML XML検証: {D3508630-5900-4276-8680-4D969FE36771} および returnToZoom="1" が含まれること
    let pkg = ibis_ooxml::Package::open(saved.clone()).expect("open package");
    let xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(
        xml.contains("{D3508630-5900-4276-8680-4D969FE36771}"),
        "must contain Zoom ext URI"
    );
    assert!(
        xml.contains("returnToZoom=\"1\""),
        "must contain returnToZoom=1 attribute"
    );

    // 再読込ラウンドトリップ検証
    let reopened = ibis_pptx::open(saved).expect("reopen pptx");
    let re_slide1 = &reopened.presentation.slides[0];
    let re_zoom = &re_slide1.shapes[base_count];
    assert!(
        re_slide1.shape_return_to_zoom.contains(&re_zoom.id),
        "return_to_zoom must be preserved after reload"
    );

    // Return to Zoom を解除して再保存
    let saved_cleared = s.run(vec![Command::SetShapeReturnToZoom {
        slide: slide1_id,
        shape: zoom_id,
        return_to_zoom: false,
    }]);

    let pkg_cleared = ibis_ooxml::Package::open(saved_cleared.clone()).expect("open cleared package");
    let xml_cleared = String::from_utf8(pkg_cleared.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(
        !xml_cleared.contains("{D3508630-5900-4276-8680-4D969FE36771}"),
        "Zoom URI must be removed when cleared"
    );

    let reopened_cleared = ibis_pptx::open(saved_cleared).expect("reopen cleared pptx");
    let re_slide1_cleared = &reopened_cleared.presentation.slides[0];
    assert!(
        !re_slide1_cleared.shape_return_to_zoom.contains(&zoom_id),
        "return_to_zoom must be cleared after reload"
    );

    // Undo / Redo の検証
    s.document.undo(); // return_to_zoom: true に戻る
    assert!(s.p().slides[0].shape_return_to_zoom.contains(&zoom_id));
    s.document.undo(); // 図形挿入・リンク設定直前
    assert!(!s.p().slides[0].shape_return_to_zoom.contains(&zoom_id));
    s.document.redo(); // 図形挿入・リンク設定
    s.document.redo(); // return_to_zoom: false
    assert!(!s.p().slides[0].shape_return_to_zoom.contains(&zoom_id));
}

/// PPTX-TRANS-017: ズーム画像の変更（Change Zoom Image）
/// ズーム図形（最初 AutoShape）のカスタム画像への差し替え（ShapeKind::Picture）、
/// OOXML <p:pic>・画像リレーション・ズームリンク・Return to Zoom の保持、
/// 画像リセットによる元 AutoShape への復帰、Undo/Redo の検証。
#[test]
fn test_change_zoom_image_and_round_trip() {
    use ibis_doc_model::{MediaFormat, Picture, SlideLink};
    let mut s = Scenario::open("change-zoom-image", "editing/pptx-plan.pptx");
    assert!(s.p().slides.len() >= 2, "must have at least 2 slides");
    let slide1_id = s.p().slides[0].id;
    let slide2_id = s.p().slides[1].id;
    let base_count = s.p().slides[0].shapes.len();

    // 1. ズーム図形（AutoShape）を作成・挿入し、ズームリンクと Return to Zoom を設定
    let zoom_id = s.document.ids().new_shape_id();
    let frame = EmuRect::new(1_200_000, 1_200_000, 2_400_000, 1_600_000);
    let original_auto = ibis_doc_model::AutoShape::new(
        ibis_doc_model::Geometry::Rect,
        ibis_doc_model::Fill::Solid(ibis_doc_model::Color::WHITE),
        None,
    );
    let shape = Shape::new(zoom_id, frame, ShapeKind::Auto(original_auto.clone()));

    s.run(vec![
        Command::InsertShape {
            slide: slide1_id,
            index: base_count,
            shape,
        },
        Command::SetShapeLink {
            slide: slide1_id,
            shape: zoom_id,
            link: Some(SlideLink::Slide(slide2_id)),
        },
        Command::SetShapeReturnToZoom {
            slide: slide1_id,
            shape: zoom_id,
            return_to_zoom: true,
        },
    ]);

    // 2. ズーム画像をカスタム画像に変更（ShapeKind::Picture）
    let img_data = png(80, 60, [0x22, 0x88, 0xcc]);
    let media = s.document.ids().add_media(MediaFormat::Png, img_data.clone());
    let picture = Picture::new(media);

    let saved = s.run(vec![Command::SetKind {
        slide: slide1_id,
        shape: zoom_id,
        kind: Box::new(ShapeKind::Picture(picture)),
    }]);

    // 3. OOXML XML検証: <p:pic> が存在し、ズームリンク・Return to Zoom が維持されていること
    let pkg = ibis_ooxml::Package::open(saved.clone()).expect("open package");
    let xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(
        xml.contains("<p:pic>") || xml.contains("p:pic"),
        "must contain <p:pic> for changed zoom image shape"
    );
    assert!(
        xml.contains("hlinkClick"),
        "must contain hlinkClick for zoom navigation link"
    );
    assert!(
        xml.contains("returnToZoom=\"1\""),
        "must contain returnToZoom=1 attribute"
    );

    // 4. 再読込ラウンドトリップ検証: ShapeKind::Picture として復元され、画像・リンク・Return to Zoom が保持されること
    let reopened = ibis_pptx::open(saved).expect("reopen pptx");
    let re_slide1 = &reopened.presentation.slides[0];
    let re_zoom = &re_slide1.shapes[base_count];
    assert!(
        matches!(re_zoom.kind, ShapeKind::Picture(_)),
        "reopened shape must be Picture"
    );
    assert_eq!(
        re_slide1.shape_links.get(&re_zoom.id),
        Some(&SlideLink::Slide(slide2_id)),
        "zoom link must be preserved"
    );
    assert!(
        re_slide1.shape_return_to_zoom.contains(&re_zoom.id),
        "return_to_zoom must be preserved"
    );

    // 5. ズーム画像をリセット（AutoShape に戻す）
    let saved_reset = s.run(vec![Command::SetKind {
        slide: slide1_id,
        shape: zoom_id,
        kind: Box::new(ShapeKind::Auto(original_auto)),
    }]);

    let reopened_reset = ibis_pptx::open(saved_reset).expect("reopen reset pptx");
    let re_slide1_reset = &reopened_reset.presentation.slides[0];
    let re_zoom_reset = &re_slide1_reset.shapes[base_count];
    assert!(
        matches!(re_zoom_reset.kind, ShapeKind::Auto(_)),
        "reset shape must be AutoShape"
    );
    assert_eq!(
        re_slide1_reset.shape_links.get(&re_zoom_reset.id),
        Some(&SlideLink::Slide(slide2_id)),
        "zoom link must remain after reset"
    );
    assert!(
        re_slide1_reset.shape_return_to_zoom.contains(&re_zoom_reset.id),
        "return_to_zoom must remain after reset"
    );

    // 6. Undo / Redo の検証
    s.document.undo(); // AutoShape から Picture（画像変更状態）に戻る
    assert!(matches!(s.p().slides[0].shape(zoom_id).unwrap().kind, ShapeKind::Picture(_)));
    s.document.undo(); // 図形挿入直後の AutoShape 状態に戻る
    assert!(matches!(s.p().slides[0].shape(zoom_id).unwrap().kind, ShapeKind::Auto(_)));
    s.document.redo(); // Picture（画像変更状態）
    assert!(matches!(s.p().slides[0].shape(zoom_id).unwrap().kind, ShapeKind::Picture(_)));
    s.document.redo(); // AutoShape（リセット状態）
    assert!(matches!(s.p().slides[0].shape(zoom_id).unwrap().kind, ShapeKind::Auto(_)));
}

/// PPTX-TRANS-018: ズームの背景と継続時間（Zoom Background & Duration）
/// OOXML <p16:zoom showBg="0" dur="1500"> 出力、
/// 再読込ラウンドトリップ保持、属性値変更（showBg="1", dur="2500"）、Undo/Redo の検証。
#[test]
fn test_zoom_background_and_duration_round_trip() {
    use ibis_doc_model::SlideLink;
    let mut s = Scenario::open("zoom-bg-duration", "editing/pptx-plan.pptx");
    assert!(s.p().slides.len() >= 2, "must have at least 2 slides");
    let slide1_id = s.p().slides[0].id;
    let slide2_id = s.p().slides[1].id;
    let base_count = s.p().slides[0].shapes.len();

    // 1. スライドズーム図形を挿入し、背景透過（showBg=false）と継続時間1500ms（dur=1500）を設定
    let zoom_id = s.document.ids().new_shape_id();
    let zoom_shape = Shape::new(
        zoom_id,
        EmuRect::new(200_000, 200_000, 1_600_000, 900_000),
        ShapeKind::Auto(ibis_doc_model::AutoShape::new(
            ibis_doc_model::Geometry::Rect,
            ibis_doc_model::Fill::Solid(ibis_doc_model::Color::WHITE),
            None,
        )),
    );

    let saved = s.run(vec![
        Command::InsertShape {
            slide: slide1_id,
            index: base_count,
            shape: zoom_shape,
        },
        Command::SetShapeLink {
            slide: slide1_id,
            shape: zoom_id,
            link: Some(SlideLink::Slide(slide2_id)),
        },
        Command::SetShapeZoomShowBg {
            slide: slide1_id,
            shape: zoom_id,
            show_bg: Some(false),
        },
        Command::SetShapeZoomDuration {
            slide: slide1_id,
            shape: zoom_id,
            duration_ms: Some(1500),
        },
    ]);

    // 2. OOXML XML検証: {D3508630-5900-4276-8680-4D969FE36771} および showBg="0", dur="1500" が含まれること
    let pkg = ibis_ooxml::Package::open(saved.clone()).expect("open package");
    let xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(
        xml.contains("{D3508630-5900-4276-8680-4D969FE36771}"),
        "slide1.xml must contain Zoom extension URI"
    );
    assert!(
        xml.contains("showBg=\"0\""),
        "slide1.xml must contain showBg=\"0\" when background is transparent"
    );
    assert!(
        xml.contains("dur=\"1500\""),
        "slide1.xml must contain dur=\"1500\""
    );

    // 3. 再読込ラウンドトリップ検証
    let reopened = ibis_pptx::open(saved).expect("reopen pptx");
    let re_slide1 = &reopened.presentation.slides[0];
    let re_zoom = &re_slide1.shapes[base_count];
    assert_eq!(
        re_slide1.shape_zoom_show_bg.get(&re_zoom.id),
        Some(&false),
        "zoom show_bg must be false after reload"
    );
    assert_eq!(
        re_slide1.shape_zoom_durations.get(&re_zoom.id),
        Some(&1500),
        "zoom duration must be 1500ms after reload"
    );

    // 4. 背景通常表示（showBg=true）に戻し、継続時間を 2500ms に変更して再保存
    let saved_modified = s.run(vec![
        Command::SetShapeZoomShowBg {
            slide: slide1_id,
            shape: zoom_id,
            show_bg: Some(true),
        },
        Command::SetShapeZoomDuration {
            slide: slide1_id,
            shape: zoom_id,
            duration_ms: Some(2500),
        },
    ]);

    let pkg_modified = ibis_ooxml::Package::open(saved_modified.clone()).expect("open modified package");
    let xml_modified = String::from_utf8(pkg_modified.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(
        xml_modified.contains("showBg=\"1\""),
        "slide1.xml must contain showBg=\"1\" when background is shown"
    );
    assert!(
        xml_modified.contains("dur=\"2500\""),
        "slide1.xml must contain dur=\"2500\""
    );

    let reopened_modified = ibis_pptx::open(saved_modified).expect("reopen modified pptx");
    let re_slide1_modified = &reopened_modified.presentation.slides[0];
    assert_eq!(
        re_slide1_modified.shape_zoom_show_bg.get(&re_zoom.id),
        Some(&true),
        "zoom show_bg must be true after reload"
    );
    assert_eq!(
        re_slide1_modified.shape_zoom_durations.get(&re_zoom.id),
        Some(&2500),
        "zoom duration must be 2500ms after reload"
    );

    // 5. Undo / Redo の検証
    s.document.undo(); // showBg=false, dur=1500 に戻る
    assert_eq!(s.p().slides[0].shape_zoom_show_bg.get(&zoom_id), Some(&false));
    assert_eq!(s.p().slides[0].shape_zoom_durations.get(&zoom_id), Some(&1500));
    s.document.undo(); // 挿入直後（デフォルト未登録）
    assert_eq!(s.p().slides[0].shape_zoom_show_bg.get(&zoom_id), None);
    assert_eq!(s.p().slides[0].shape_zoom_durations.get(&zoom_id), None);
    s.document.redo(); // showBg=false, dur=1500
    assert_eq!(s.p().slides[0].shape_zoom_show_bg.get(&zoom_id), Some(&false));
    assert_eq!(s.p().slides[0].shape_zoom_durations.get(&zoom_id), Some(&1500));
    s.document.redo(); // showBg=true, dur=2500
    assert_eq!(s.p().slides[0].shape_zoom_show_bg.get(&zoom_id), Some(&true));
    assert_eq!(s.p().slides[0].shape_zoom_durations.get(&zoom_id), Some(&2500));
}

/// PPTX-TRANS-013: サマリーズーム（Summary Zoom）
/// 複数スライドに対するサマリーズームカード群の挿入、
/// OOXML <a:hlinkClick> / <p16:zoom returnToZoom="1"> 出力、
/// 再読込ラウンドトリップ保持、Undo/Redo の検証。
#[test]
fn test_summary_zoom_shapes_and_round_trip() {
    use ibis_doc_model::SlideLink;
    let mut s = Scenario::open("summary-zoom", "editing/pptx-plan.pptx");
    assert!(s.p().slides.len() >= 4, "must have at least 4 slides");
    let slide1_id = s.p().slides[0].id;
    let target_ids = vec![s.p().slides[1].id, s.p().slides[2].id, s.p().slides[3].id];
    let base_count = s.p().slides[0].shapes.len();

    let mut commands = Vec::new();
    let mut zoom_ids = Vec::new();

    for (i, &target_id) in target_ids.iter().enumerate() {
        let zoom_id = s.document.ids().new_shape_id();
        let frame = EmuRect::new(
            1_000_000 + (i as i64 * 2_200_000),
            2_000_000,
            2_000_000,
            1_200_000,
        );
        let shape = Shape::new(
            zoom_id,
            frame,
            ShapeKind::Auto(ibis_doc_model::AutoShape::new(
                ibis_doc_model::Geometry::Rect,
                ibis_doc_model::Fill::Solid(ibis_doc_model::Color::WHITE),
                None,
            )),
        );
        commands.push(Command::InsertShape {
            slide: slide1_id,
            index: base_count + i,
            shape,
        });
        commands.push(Command::SetShapeLink {
            slide: slide1_id,
            shape: zoom_id,
            link: Some(SlideLink::Slide(target_id)),
        });
        commands.push(Command::SetShapeReturnToZoom {
            slide: slide1_id,
            shape: zoom_id,
            return_to_zoom: true,
        });
        zoom_ids.push(zoom_id);
    }

    let saved = s.run(commands);

    // OOXML XML検証
    let pkg = ibis_ooxml::Package::open(saved.clone()).expect("open package");
    let xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(
        xml.contains("returnToZoom=\"1\""),
        "slide1.xml must contain returnToZoom=\"1\" for summary zoom cards"
    );
    assert!(
        xml.contains("ppaction://hlinksldjump"),
        "slide1.xml must contain jump links for summary zoom cards"
    );

    // 再読込ラウンドトリップ検証
    let reopened = ibis_pptx::open(saved).expect("reopen pptx");
    let re_slide1 = &reopened.presentation.slides[0];
    for (i, &target_id) in target_ids.iter().enumerate() {
        let re_shape = &re_slide1.shapes[base_count + i];
        assert_eq!(
            re_slide1.shape_links.get(&re_shape.id),
            Some(&SlideLink::Slide(target_id)),
            "target slide link must be preserved"
        );
        assert!(
            re_slide1.shape_return_to_zoom.contains(&re_shape.id),
            "return_to_zoom must be preserved for all summary cards"
        );
    }

    // Undo / Redo 検証
    s.document.undo(); // 挿入前に戻る
    assert_eq!(s.p().slides[0].shapes.len(), base_count);
    s.document.redo(); // 挿入後に戻る
    assert_eq!(s.p().slides[0].shapes.len(), base_count + target_ids.len());
    for &z_id in &zoom_ids {
        assert!(s.p().slides[0].shape_return_to_zoom.contains(&z_id));
    }
}

/// PPTX-CHART-006: 組合せと第2軸（Combo Chart & Secondary Axis）
/// 集合縦棒 - 折れ線、集合縦棒 - 折れ線 (第 2 軸)、積み上げ面 - 集合縦棒の挿入、
/// 複数チャートグループ・第2数値軸の OOXML シリアライズ、再読込保持、Undo/Redo を検証。
#[test]
fn test_combo_charts_insert_save_and_reopen() {
    use ibis_pptx::NewChartKind;
    use ibis_doc_model::{ChartType, Side};

    let mut s = Scenario::open("combo-charts", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let base_count = s.p().slides[0].shapes.len();

    // 1. ComboColumnLine（集合縦棒 - 折れ線）
    let id1 = s.document.ids().new_shape_id();
    let current = s.p().clone();
    let shape1 = s.opened.source.new_chart_kind(
        &current,
        slide,
        id1,
        EmuRect::new(500_000, 500_000, 4_000_000, 3_000_000),
        NewChartKind::ComboColumnLine,
    ).unwrap();
    let ShapeKind::Chart(chart1) = &shape1.kind else { panic!("chart expected") };
    assert_eq!(chart1.groups.len(), 2, "ComboColumnLine has 2 chart groups");
    assert!(matches!(chart1.groups[0].kind, ChartType::Bar { horizontal: false, .. }));
    assert!(matches!(chart1.groups[1].kind, ChartType::Line));
    assert_eq!(chart1.groups[0].series.len(), 2, "bar group has 2 series");
    assert_eq!(chart1.groups[1].series.len(), 1, "line group has 1 series");

    // 2. ComboColumnLineSecondaryAxis（集合縦棒 - 折れ線 (第 2 軸)）
    let id2 = s.document.ids().new_shape_id();
    let shape2 = s.opened.source.new_chart_kind(
        &current,
        slide,
        id2,
        EmuRect::new(5_000_000, 500_000, 4_000_000, 3_000_000),
        NewChartKind::ComboColumnLineSecondaryAxis,
    ).unwrap();
    let ShapeKind::Chart(chart2) = &shape2.kind else { panic!("chart expected") };
    assert_eq!(chart2.groups.len(), 2);
    assert_eq!(chart2.axes.len(), 3, "has category axis, primary value axis, secondary value axis");
    assert!(chart2.axes.iter().any(|a| a.side == Side::Right), "secondary value axis on right");

    // 3. ComboAreaColumn（積み上げ面 - 集合縦棒）
    let id3 = s.document.ids().new_shape_id();
    let shape3 = s.opened.source.new_chart_kind(
        &current,
        slide,
        id3,
        EmuRect::new(500_000, 4_000_000, 4_000_000, 3_000_000),
        NewChartKind::ComboAreaColumn,
    ).unwrap();
    let ShapeKind::Chart(chart3) = &shape3.kind else { panic!("chart expected") };
    assert_eq!(chart3.groups.len(), 2);
    assert!(matches!(chart3.groups[0].kind, ChartType::Area));
    assert!(matches!(chart3.groups[1].kind, ChartType::Bar { horizontal: false, .. }));

    // まとめて挿入・保存
    let saved = s.run(vec![
        Command::InsertShape { slide, index: base_count, shape: shape1 },
        Command::InsertShape { slide, index: base_count + 1, shape: shape2 },
        Command::InsertShape { slide, index: base_count + 2, shape: shape3 },
    ]);

    // OOXML XML検証
    let pkg = ibis_ooxml::Package::open(saved.clone()).expect("open package");
    let chart_parts: Vec<_> = pkg.names().filter(|p| p.starts_with("ppt/charts/chart") && p.ends_with(".xml")).collect();
    assert!(chart_parts.len() >= 3, "at least 3 charts saved");

    let mut found_bar_line = false;
    let mut found_secondary_ax = false;
    let mut found_area_bar = false;
    for part in &chart_parts {
        let xml = String::from_utf8(pkg.read(part).unwrap().to_vec()).unwrap();
        if xml.contains("<c:barChart>") && xml.contains("<c:lineChart>") {
            found_bar_line = true;
            if xml.contains("val=\"r\"") {
                found_secondary_ax = true;
            }
        }
        if xml.contains("<c:areaChart>") && xml.contains("<c:barChart>") {
            found_area_bar = true;
        }
    }
    assert!(found_bar_line, "must contain combo bar and line chart xml");
    assert!(found_secondary_ax, "must contain secondary axis (val=\"r\") xml");
    assert!(found_area_bar, "must contain combo area and bar chart xml");

    // 再オープン検証
    let reopened = ibis_pptx::open(saved).expect("reopen saved pptx");
    let shapes = &reopened.presentation.slides[0].shapes;
    let charts: Vec<_> = shapes.iter().filter_map(|s| match &s.kind {
        ShapeKind::Chart(c) => Some(c.as_ref()),
        _ => None,
    }).collect();
    assert!(charts.len() >= 3, "at least 3 charts found on slide");

    let r_combo_bar_line = charts.iter().find(|c| {
        c.groups.len() == 2
            && matches!(c.groups[0].kind, ChartType::Bar { horizontal: false, .. })
            && matches!(c.groups[1].kind, ChartType::Line)
            && !c.axes.iter().any(|a| a.side == Side::Right)
    }).expect("combo column line chart reopened");
    assert_eq!(r_combo_bar_line.groups[0].series.len(), 2);
    assert_eq!(r_combo_bar_line.groups[1].series.len(), 1);

    let r_combo_sec = charts.iter().find(|c| {
        c.groups.len() == 2
            && matches!(c.groups[0].kind, ChartType::Bar { horizontal: false, .. })
            && matches!(c.groups[1].kind, ChartType::Line)
            && c.axes.iter().any(|a| a.side == Side::Right)
    }).expect("combo column line with secondary axis reopened");
    assert_eq!(r_combo_sec.axes.len(), 3);

    let r_combo_area_bar = charts.iter().find(|c| {
        c.groups.len() == 2
            && matches!(c.groups[0].kind, ChartType::Area)
            && matches!(c.groups[1].kind, ChartType::Bar { horizontal: false, .. })
    }).expect("combo area column chart reopened");
    assert_eq!(r_combo_area_bar.groups[0].series.len(), 2);
    assert_eq!(r_combo_area_bar.groups[1].series.len(), 1);

    // Undo / Redo 検証
    s.document.undo();
    assert_eq!(s.p().slides[0].shapes.len(), base_count);
    s.document.redo();
    assert_eq!(s.p().slides[0].shapes.len(), base_count + 3);
}

/// PPTX-CHART-005: 株価・レーダー・表面（Stock, Radar & Surface Charts）
/// 株価（HLC, OHLC, VHLC, VOHLC）、レーダー（通常, 塗りつぶし）、
/// 3D表面（通常, ワイヤーフレーム）、等高線（通常, ワイヤーフレーム）の新規作成、
/// OOXML差分保存、再読込保持、Undo/Redo を検証。
#[test]
fn test_stock_radar_and_surface_charts_round_trip() {
    use ibis_pptx::NewChartKind;
    use ibis_doc_model::{ChartType, RadarStyle};

    let mut s = Scenario::open("stock-radar-surface", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let base_count = s.p().slides[0].shapes.len();

    let kinds = [
        (NewChartKind::StockHLC, "StockHLC"),
        (NewChartKind::StockOHLC, "StockOHLC"),
        (NewChartKind::StockVHLC, "StockVHLC"),
        (NewChartKind::StockVOHLC, "StockVOHLC"),
        (NewChartKind::Radar, "Radar"),
        (NewChartKind::RadarFilled, "RadarFilled"),
        (NewChartKind::Surface3D, "Surface3D"),
        (NewChartKind::Surface3DWireframe, "Surface3DWireframe"),
        (NewChartKind::Contour, "Contour"),
        (NewChartKind::ContourWireframe, "ContourWireframe"),
    ];

    let mut inserted_shapes = Vec::new();
    let current = s.p().clone();
    for (i, (kind, name)) in kinds.iter().enumerate() {
        let shape_id = s.document.ids().new_shape_id();
        let frame = EmuRect::new(500_000 * (i as i64 + 1), 500_000, 3_000_000, 2_000_000);
        let shape = s.opened.source.new_chart_kind(&current, slide, shape_id, frame, *kind)
            .unwrap_or_else(|e| panic!("failed to create {name}: {e}"));
        inserted_shapes.push(shape);
    }

    // 挿入コマンドを一括実行して保存
    let commands: Vec<_> = inserted_shapes.into_iter().enumerate().map(|(idx, shape)| {
        Command::InsertShape { slide, index: base_count + idx, shape }
    }).collect();

    let saved = s.run(commands);

    // OOXML XML要素検証
    let pkg = ibis_ooxml::Package::open(saved.clone()).expect("valid package");
    let chart_parts: Vec<_> = pkg.names().filter(|n| n.starts_with("ppt/charts/chart") && n.ends_with(".xml")).collect();
    assert!(chart_parts.len() >= kinds.len(), "all chart parts saved");

    let all_xml = chart_parts.iter().map(|p| String::from_utf8(pkg.read(p).unwrap().to_vec()).unwrap()).collect::<Vec<_>>().join("\n");
    assert!(all_xml.contains("<c:stockChart>"), "contains stockChart");
    assert!(all_xml.contains("<c:radarChart>"), "contains radarChart");
    assert!(all_xml.contains("<c:surface3DChart>"), "contains surface3DChart");
    assert!(all_xml.contains("<c:surfaceChart>"), "contains surfaceChart");
    assert!(all_xml.contains(r#"radarStyle val="filled""#), "contains filled radar");
    assert!(all_xml.contains(r#"wireframe val="1""#), "contains wireframe");

    // 再オープン検証
    let reopened = ibis_pptx::open(saved).expect("reopen saved presentation");
    let re_charts: Vec<_> = reopened.presentation.slides[0].shapes.iter().filter_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some(c.as_ref()),
        _ => None,
    }).collect();
    assert!(re_charts.len() >= kinds.len());

    // 各グラフ種別の保持確認
    assert!(re_charts.iter().any(|c| c.groups.iter().any(|g| matches!(g.kind, ChartType::Stock { up_down_bars: false, hi_low_lines: true, .. }))));
    assert!(re_charts.iter().any(|c| c.groups.iter().any(|g| matches!(g.kind, ChartType::Stock { up_down_bars: true, hi_low_lines: true, .. }))));
    assert!(re_charts.iter().any(|c| c.groups.iter().any(|g| matches!(g.kind, ChartType::Radar { style: Some(RadarStyle::Filled) }))));
    assert!(re_charts.iter().any(|c| c.groups.iter().any(|g| matches!(g.kind, ChartType::Surface { is_3d: true, wireframe: false }))));
    assert!(re_charts.iter().any(|c| c.groups.iter().any(|g| matches!(g.kind, ChartType::Surface { is_3d: true, wireframe: true }))));
    assert!(re_charts.iter().any(|c| c.groups.iter().any(|g| matches!(g.kind, ChartType::Surface { is_3d: false, wireframe: false }))));
    assert!(re_charts.iter().any(|c| c.groups.iter().any(|g| matches!(g.kind, ChartType::Surface { is_3d: false, wireframe: true }))));

    // Undo / Redo 検証
    s.document.undo();
    assert_eq!(s.p().slides[0].shapes.len(), base_count);
    s.document.redo();
    assert_eq!(s.p().slides[0].shapes.len(), base_count + kinds.len());
}

/// PPTX-THEME-002: retheme selected slides only without altering presentation-wide theme.
#[test]
fn retheme_selected_slides() {
    let mut s = Scenario::open("retheme-selected-slides", "editing/pptx-plan.pptx");
    let pres = s.p().clone();
    assert!(pres.slides.len() >= 2);
    let slide0_orig = pres.slides[0].clone();
    let slide1_orig = pres.slides[1].clone();
    let theme_colors_orig = pres.theme_colors.clone();

    // Aspect colors
    let aspect_colors = vec![
        ("dark1".to_string(), ibis_doc_model::Rgb(0x000000)),
        ("light1".to_string(), ibis_doc_model::Rgb(0xffffff)),
        ("dark2".to_string(), ibis_doc_model::Rgb(0x323232)),
        ("light2".to_string(), ibis_doc_model::Rgb(0xd1dad6)),
        ("accent1".to_string(), ibis_doc_model::Rgb(0xf0a22e)),
        ("accent2".to_string(), ibis_doc_model::Rgb(0x9fc299)),
        ("accent3".to_string(), ibis_doc_model::Rgb(0x4a7a96)),
        ("accent4".to_string(), ibis_doc_model::Rgb(0x948b54)),
        ("accent5".to_string(), ibis_doc_model::Rgb(0x6a897b)),
        ("accent6".to_string(), ibis_doc_model::Rgb(0xabc3c9)),
        ("hlink".to_string(), ibis_doc_model::Rgb(0x6b9f25)),
        ("folHlink".to_string(), ibis_doc_model::Rgb(0xb54001)),
    ];

    let target_slide = pres.slides[2].clone();
    let target_id = target_slide.id;

    let cmds = s.opened.source.retheme_slides(s.p(), &[target_id], aspect_colors, None).expect("retheme_slides succeeds");
    assert!(!cmds.is_empty(), "commands generated for selected slide");
    let saved = s.run(cmds);

    // 他のスライド（slide 0, slide 1）は元のまま、対象スライド（slide 2）のみ配色が変更されている
    assert_eq!(s.p().slides[0], slide0_orig, "slide 0 unchanged");
    assert_eq!(s.p().slides[1], slide1_orig, "slide 1 unchanged");
    assert_ne!(s.p().slides[2], target_slide, "slide 2 changed");
    assert_eq!(s.p().theme_colors, theme_colors_orig, "presentation theme_colors unchanged");

    // 保存・再オープン検証
    let reopened = ibis_pptx::open(saved).expect("reopen succeeds");
    assert_eq!(reopened.presentation.theme_colors, theme_colors_orig, "reopened presentation theme_colors preserved");
    assert_eq!(reopened.presentation.slides[0].shapes.len(), slide0_orig.shapes.len());
    assert_eq!(reopened.presentation.slides[1].shapes.len(), slide1_orig.shapes.len());
    assert_eq!(reopened.presentation.slides[2].shapes.len(), target_slide.shapes.len());
}
















/// スピーカー ノート: a paragraph whose text stays keeps its run formatting
/// when another line of the notes is changed or added.
#[test]
fn speaker_notes_keep_run_formatting() {
    let mut s = Scenario::open("notes-format", "editing/pptx-plan.pptx");
    let (index, _) = s.p().slides.iter().enumerate().find(|(_, sl)| !sl.notes.is_empty()).unwrap();
    let slide = s.p().slides[index].id;
    // Make the first run of the notes page bold.
    let mut package = ibis_ooxml::Package::open(s.data.clone()).unwrap();
    {
        let xml = package.xml_mut("ppt/notesSlides/notesSlide1.xml").unwrap();
        let root = xml.root_element().unwrap();
        let run = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "r").unwrap();
        let pr = xml.child(run, "rPr").unwrap();
        xml.set_attr(pr, "b", "1");
    }
    s.data = package.save(SaveOptions::default()).unwrap();
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    let notes = s.p().slides[index].notes.clone();
    let saved = s.run(vec![Command::SetNotes { slide, notes: format!("{notes}\n追加の行") }]);
    let package = ibis_ooxml::Package::open(saved).unwrap();
    let xml = package.xml("ppt/notesSlides/notesSlide1.xml").unwrap();
    let root = xml.root_element().unwrap();
    let bold = xml.descendants(root).into_iter().filter(|&n| xml.is_element(n) && xml.local_name(n) == "rPr" && xml.attr(n, "b") == Some("1")).count();
    assert_eq!(bold, 1, "the bold run of the unchanged paragraph is kept");
}

/// レイアウト名の変更: written as the layout's name, undone to the original file.
#[test]
fn rename_layout() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("layout-rename", file);
        let layout = s.p().layouts[0].id;
        s.run(vec![Command::RenameLayout { layout, name: "名前を変えたレイアウト & <1>".into() }]);
    }
}

/// レイアウトの複製: a copy is a new layout part in its master, with the
/// name given, that a slide can follow, and the original is left as it was.
#[test]
fn duplicate_layout() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("layout-copy", file);
        let before = s.p().layouts.clone();
        let source = before[0].clone();
        let copy = ibis_doc_model::Layout { id: ibis_doc_model::LayoutId(before.iter().map(|l| l.id.0).max().unwrap() + 1), name: format!("{} のコピー", source.name), copy_of: Some(source.id), ..source.clone() };
        let slide = s.p().slides[0].id;
        s.document.execute(vec![Command::InsertLayout { index: before.len(), layout: copy.clone() }]).unwrap();
        let followed = s.p().with_layout(slide, copy.id).unwrap();
        s.document.execute(vec![Command::SetSlide { slide: followed }]).unwrap();
        assert!(s.document.undo() && s.document.undo() && s.document.redo() && s.document.redo());
        let saved = s.opened.source.save(s.p(), SaveOptions::default()).unwrap();
        let reread = ibis_pptx::read(&saved).unwrap();
        assert_eq!(reread.layouts.len(), before.len() + 1, "{file}");
        let (mut a, mut b): (Vec<_>, Vec<_>) = (reread.layouts.iter().map(|l| l.name.clone()).collect(), before.iter().map(|l| l.name.clone()).chain([copy.name.clone()]).collect());
        a.sort();
        b.sort();
        assert_eq!(a, b, "{file}: the layouts, and the copy");
        let new = reread.layouts.iter().find(|l| l.name == copy.name).unwrap();
        assert_eq!(new.placeholders.len(), source.placeholders.len());
        assert_eq!(reread.slides[0].layout, Some(new.id), "the slide follows the copy");
        // Undone, the original file.
        let mut undone = s.document.clone();
        while undone.undo() {}
        assert!(s.opened.source.save(undone.presentation(), SaveOptions::default()).unwrap() == s.data, "{file}");
    }
}

/// The commands that copy master `source` of `p` as `name`, with its layouts.
fn copy_master_commands(p: &Presentation, source: ibis_doc_model::MasterId, name: &str) -> (ibis_doc_model::MasterId, Vec<Command>) {
    let top = p.layouts.iter().map(|l| l.id.0).chain(p.masters.iter().map(|m| m.id.0)).max().unwrap();
    let id = ibis_doc_model::MasterId(top + 1);
    let mut commands = vec![Command::InsertMaster { index: p.masters.len(), master: ibis_doc_model::Master { id, name: name.into(), copy_of: Some(source) } }];
    for (i, l) in p.layouts.iter().filter(|l| l.master == Some(source)).enumerate() {
        let layout = ibis_doc_model::Layout { id: ibis_doc_model::LayoutId(top + 2 + i as u64), copy_of: Some(l.id), master: Some(id), ..l.clone() };
        commands.push(Command::InsertLayout { index: p.layouts.len() + i, layout });
    }
    (id, commands)
}

/// マスター名の変更: the theme's name, undone to the original file.
#[test]
fn rename_master() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("master-rename", file);
        assert!(!s.p().masters.is_empty());
        let master = s.p().masters[0].id;
        let saved = s.run(vec![Command::RenameMaster { master, name: "別のテーマ & <名>".into() }]);
        assert_eq!(ibis_pptx::read(&saved).unwrap().masters[0].name, "別のテーマ & <名>");
    }
}

/// マスターの複製と削除: a copy has its own theme and layouts that a slide
/// can follow; removed (with its layouts), the file has the one master again.
#[test]
fn duplicate_and_remove_master() {
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("master-copy", file);
        let (before_masters, before_layouts) = (s.p().masters.len(), s.p().layouts.len());
        let source = s.p().masters[0].id;
        let (copy, commands) = copy_master_commands(s.p(), source, "複製したマスター");
        let in_master = s.p().layouts.iter().filter(|l| l.master == Some(source)).count();
        s.document.execute(commands).unwrap();
        let slide = s.p().slides[0].id;
        let follows = s.p().layouts.iter().find(|l| l.master == Some(copy)).unwrap().id;
        let followed = s.p().with_layout(slide, follows).unwrap();
        s.document.execute(vec![Command::SetSlide { slide: followed }]).unwrap();
        let saved = s.opened.source.save(s.p(), SaveOptions::default()).unwrap();
        let reread = ibis_pptx::read(&saved).unwrap();
        assert_eq!(reread.masters.len(), before_masters + 1, "{file}");
        let new = reread.masters.iter().find(|m| m.name == "複製したマスター").expect("the copy's name");
        assert_eq!(reread.layouts.iter().filter(|l| l.master == Some(new.id)).count(), in_master, "{file}: the copy's layouts");
        let slide_layout = reread.layout(reread.slides[0].layout.unwrap()).unwrap();
        assert_eq!(slide_layout.master, Some(new.id), "the slide follows the copy");
        // Undone, the original file.
        let mut undone = s.document.clone();
        while undone.undo() {}
        assert!(s.opened.source.save(undone.presentation(), SaveOptions::default()).unwrap() == s.data, "{file}");
        // The slide back on the first master, the copy removed.
        let mut reopened = Scenario { name: "master-remove".into(), opened: ibis_pptx::open(saved.clone()).unwrap(), data: saved.clone(), document: Document::new(reread.clone()) };
        let extra = reopened.p().masters.iter().find(|m| m.name == "複製したマスター").unwrap().id;
        let first = reopened.p().layouts.iter().find(|l| l.master != Some(extra)).unwrap().id;
        let back = reopened.p().with_layout(reopened.p().slides[0].id, first).unwrap();
        let mut remove = vec![Command::SetSlide { slide: back }];
        for l in reopened.p().layouts.iter().filter(|l| l.master == Some(extra)) {
            remove.push(Command::RemoveLayout { layout: l.id });
        }
        remove.push(Command::RemoveMaster { master: extra });
        reopened.document.execute(remove).unwrap();
        let saved = reopened.opened.source.save(reopened.p(), SaveOptions::default()).unwrap();
        let again = ibis_pptx::read(&saved).unwrap();
        assert_eq!(again.masters.len(), before_masters, "{file}");
        assert_eq!(again.layouts.len(), before_layouts, "{file}");
    }
}

/// ノート ページの図形: a shape put on a notes page (not a placeholder) is
/// read, and stays when the notes are changed.
#[test]
fn notes_page_shapes_are_read_and_kept() {
    let mut s = Scenario::open("notes-shapes", "editing/pptx-plan.pptx");
    let (index, _) = s.p().slides.iter().enumerate().find(|(_, sl)| !sl.notes.is_empty()).unwrap();
    let mut package = ibis_ooxml::Package::open(s.data.clone()).unwrap();
    {
        let xml = package.xml_mut("ppt/notesSlides/notesSlide1.xml").unwrap();
        let root = xml.root_element().unwrap();
        let tree = xml.path(root, &["cSld", "spTree"]).unwrap();
        let sp = xml.append_element(tree, "p:sp");
        let nv = xml.append_element(sp, "p:nvSpPr");
        let c = xml.append_element(nv, "p:cNvPr");
        xml.set_attr(c, "id", "9");
        xml.set_attr(c, "name", "ノートの図形");
        xml.append_element(nv, "p:cNvSpPr");
        xml.append_element(nv, "p:nvPr");
        let pr = xml.append_element(sp, "p:spPr");
        let xf = xml.append_element(pr, "a:xfrm");
        let off = xml.append_element(xf, "a:off");
        xml.set_attr(off, "x", "685800");
        xml.set_attr(off, "y", "7000000");
        let ext = xml.append_element(xf, "a:ext");
        xml.set_attr(ext, "cx", "2000000");
        xml.set_attr(ext, "cy", "500000");
        let geom = xml.append_element(pr, "a:prstGeom");
        xml.set_attr(geom, "prst", "rect");
        xml.append_element(geom, "a:avLst");
        let fill = xml.append_element(pr, "a:solidFill");
        let clr = xml.append_element(fill, "a:srgbClr");
        xml.set_attr(clr, "val", "FF0000");
    }
    s.data = package.save(SaveOptions::default()).unwrap();
    s.opened = ibis_pptx::open(s.data.clone()).unwrap();
    s.document = Document::new(s.opened.presentation.clone());
    assert_eq!(s.p().slides[index].notes_shapes.len(), 1, "the notes page's shape, and no placeholder");
    assert_eq!(s.p().notes_size.width, 6_858_000);
    let slide = s.p().slides[index].id;
    let notes = s.p().slides[index].notes.clone();
    let saved = s.opened.source.save(&{
        let mut d = s.document.clone();
        d.execute(vec![Command::SetNotes { slide, notes: format!("{notes}\n追加") }]).unwrap();
        d.presentation().clone()
    }, SaveOptions::default()).unwrap();
    let again = ibis_pptx::read(&saved).unwrap();
    assert_eq!(again.slides[index].notes_shapes.len(), 1, "kept when the notes change");
    assert!(again.slides[index].notes.ends_with("追加"));
}

/// ノートのヘッダーとフッター: the header, date, footer and number put on the
/// notes pages (made for slides without), changed, taken away.
#[test]
fn notes_header_and_footer() {
    use ibis_doc_model::{NotesDate, NotesMarks};
    for file in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open("notes-marks", file);
        assert!(s.p().notes_marks.is_empty() || s.p().notes_marks.number, "{file}: {:?}", s.p().notes_marks);
        let first = s.p().notes_marks.clone();
        let marks = NotesMarks { header: Some("ヘッダー & <1>".into()), date: Some(NotesDate::Fixed("2026/10/10".into())), footer: Some("社外秘".into()), number: true };
        let saved = s.run(vec![Command::SetNotesMarks { marks: marks.clone() }]);
        let again = ibis_pptx::read(&saved).unwrap();
        assert_eq!(again.notes_marks, marks, "{file}: read back");
        assert!(again.notes_frames.iter().any(|(k, _)| k == "hdr"), "the notes master's places");
        // Every slide has a notes page now.
        let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
        let pages = package.names().filter(|n| n.starts_with("ppt/notesSlides/notesSlide") && n.ends_with(".xml")).count();
        assert_eq!(pages, again.slides.len(), "{file}: a notes page for each slide");
        // The handout master's places say the same (the handout prints them in its margins).
        let handout = package.names().find(|n| n.starts_with("ppt/handoutMasters/handoutMaster") && n.ends_with(".xml")).map(str::to_string).expect("a handout master");
        let handout_xml = String::from_utf8(package.read(&handout).unwrap()).unwrap();
        assert!(handout_xml.contains("社外秘") && handout_xml.contains("2026/10/10") && handout_xml.contains("ヘッダー &amp; &lt;1&gt;"), "{file}: {handout_xml}");
        // Changed (the date a field), then the footer and number taken away.
        let changed = NotesMarks { header: None, date: Some(NotesDate::Auto("2026/10/10".into())), footer: None, number: false };
        s.run(vec![Command::SetNotesMarks { marks: changed.clone() }]);
        let _ = first;
        let saved = s.opened.source.save(s.p(), SaveOptions::default()).unwrap();
        assert_eq!(ibis_pptx::read(&saved).unwrap().notes_marks, changed, "{file}");
    }
}

/// 変形: the morph transition (PowerPoint 2016's `p159:morph`) is written
/// with a fade as the fallback, read back with how objects are matched, and
/// timing changes of it are kept.
#[test]
fn morph_transition_round_trip() {
    use ibis_doc_model::Transition;
    let mut s = Scenario::open("morph", "editing/pptx-plan.pptx");
    let slide = s.p().slides[1].id;
    let by_object = Transition { duration: 2000, ..Transition::new("morph") };
    let saved = s.run(vec![Command::SetTransition { slide, transition: Some(by_object.clone()) }]);
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = String::from_utf8(package.read("ppt/slides/slide2.xml").unwrap()).unwrap();
    assert!(xml.contains("morph option=\"byObject\""), "{xml}");
    assert!(xml.contains("Requires=\"p159\""), "{xml}");
    assert!(xml.contains("<p:fade/>"), "the fallback: {xml}");
    assert_eq!(ibis_pptx::read(&saved).unwrap().slides[1].transition.as_ref().map(|t| t.effect.as_str()), Some("morph"));
    // By word, then a timing change only.
    let by_word = Transition { direction: Some("byWord".into()), ..by_object.clone() };
    let saved = s.run(vec![Command::SetTransition { slide, transition: Some(by_word.clone()) }]);
    assert_eq!(ibis_pptx::read(&saved).unwrap().slides[1].transition.as_ref().and_then(|t| t.direction.clone()).as_deref(), Some("byWord"));
    s.run(vec![Command::SetTransition { slide, transition: Some(Transition { duration: 3000, ..by_word }) }]);
}

/// ファイルへのリンクの動画: a clip that points at a file outside the
/// presentation is written as `a:videoFile` related to the file (an external
/// relationship), with no copy in the package, and read back as linked.
#[test]
fn linked_video_round_trip() {
    use ibis_doc_model::Clip;
    let mut s = Scenario::open("linked-video", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let (poster, _, _) = ibis_pptx::clip_poster(false, 16.0 / 9.0);
    let poster_media = s.document.ids().add_media(MediaFormat::Png, poster);
    let video_media = s.document.ids().add_media(MediaFormat::Mp4, Vec::<u8>::new());
    let link = "file:///Users/me/Movies/my%20movie.mp4";
    let id = s.document.ids().new_shape_id();
    let picture = Picture {
        media: poster_media,
        crop: Default::default(),
        stroke: None,
        corner: 0,
        duotone: None,
        clear: None,
        luminance: None,
        clip: Some(Clip { link: Some(link.into()), ..Clip::new(video_media, false) }),
        shadow: None,
        reflection: None,
        soft_edge: None,
        alt_text: Default::default(),
        decorative: false,
        name: String::new(),
        geometry: None,
        effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None,
    };
    let index = s.p().slides[0].shapes.len();
    let shape = Shape::new(id, EmuRect::new(1_000_000, 1_000_000, 3_200_000, 1_800_000), ShapeKind::Picture(picture));
    let saved = s.opened.source.save(&{
        let mut d = s.document.clone();
        d.execute(vec![Command::InsertShape { slide, index, shape }]).unwrap();
        d.presentation().clone()
    }, SaveOptions::default()).unwrap();
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = String::from_utf8(package.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(xml.contains("videoFile"), "{xml}");
    assert!(!xml.contains("p14:media"), "no embedded copy: {xml}");
    let rels = String::from_utf8(package.read("ppt/slides/_rels/slide1.xml.rels").unwrap()).unwrap();
    assert!(rels.contains("TargetMode=\"External\"") && rels.contains("my%20movie.mp4") && rels.contains("/video\""), "{rels}");
    assert!(!package.names().any(|n| n.ends_with(".mp4")), "no copy of the file");
    let again = ibis_pptx::read(&saved).unwrap();
    let clip = again.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Picture(p) => p.clip.clone(),
        _ => None,
    }).expect("the clip");
    assert_eq!(clip.link.as_deref(), Some(link));
    assert!(!clip.audio);
    assert!(again.media(clip.media).is_some_and(|m| m.data.is_empty()), "nothing kept of the file");
    // Opened again and saved without changes: the same file.
    let reopened = ibis_pptx::open(saved.clone()).unwrap();
    assert!(reopened.source.save(&reopened.presentation, SaveOptions::default()).unwrap() == saved, "an unchanged save keeps the file");
}

/// スクリーンショット挿入 (Insert Screenshot / Window Snapshot):
/// verifies that `ibis_pptx::screenshot_snapshot` produces a valid PNG,
/// that inserting it as a picture shape saves correctly as `<p:pic>` with `image*.png` in media,
/// and that reading back the saved presentation preserves the picture shape and dimensions.
#[test]
fn test_insert_screenshot_and_save_round_trip() {
    let mut s = Scenario::open("screenshot", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let (png_bytes, width, height) = ibis_pptx::screenshot_snapshot(640, 360);
    assert_eq!(&png_bytes[0..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A], "valid PNG header");
    assert_eq!(width, 640);
    assert_eq!(height, 360);

    let media_id = s.document.ids().add_media(MediaFormat::Png, png_bytes.clone());
    let shape_id = s.document.ids().new_shape_id();
    let emu_w = 640 * 9525;
    let emu_h = 360 * 9525;
    let picture = Picture {
        media: media_id,
        crop: Default::default(),
        stroke: None,
        corner: 0,
        duotone: None,
        clear: None,
        luminance: None,
        clip: None,
        shadow: None,
        reflection: None,
        soft_edge: None,
        alt_text: Default::default(),
        decorative: false,
        name: "Screenshot_Window.png".into(),
        geometry: None,
        effects: Default::default(),
        tile: None,
        fill_rect: None,
        artistic_effect: None,
    };
    let index = s.p().slides[0].shapes.len();
    let shape = Shape::new(shape_id, EmuRect::new(1_000_000, 1_000_000, emu_w, emu_h), ShapeKind::Picture(picture));

    let saved = s.run(vec![Command::InsertShape { slide, index, shape }]);
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = String::from_utf8(package.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(xml.contains("<p:pic>") || xml.contains("<p:pic "), "{xml}");
    assert!(xml.contains("Screenshot_Window.png") || xml.contains("descr=\"\""), "{xml}");

    let has_image_png = package.names().any(|n| n.starts_with("ppt/media/image") && n.ends_with(".png"));
    assert!(has_image_png, "package contains saved screenshot media png");

    let again = ibis_pptx::read(&saved).unwrap();
    let read_pic = again.slides[0].shapes.iter().find(|sh| sh.frame == EmuRect::new(1_000_000, 1_000_000, emu_w, emu_h)).expect("the screenshot shape");
    assert_eq!(read_pic.frame, EmuRect::new(1_000_000, 1_000_000, emu_w, emu_h));
    match &read_pic.kind {
        ShapeKind::Picture(p) => {
            let m = again.media(p.media).expect("media present");
            assert_eq!(&*m.data, png_bytes.as_slice(), "media bytes match screenshot snapshot");
        }
        _ => panic!("expected Picture shape"),
    }
}

#[test]
fn numbered_list_start_at_round_trip() {
    use ibis_doc_model::{ListKind, ParagraphFormat};
    let mut s = Scenario::open("numbered-start-at", "editing/pptx-plan.pptx");
    let (slide, shape, text) = text_shapes(s.p(), 2).into_iter().next().unwrap();
    let original = s.p().clone();

    // 1. Apply numbered list starting at 5.
    let cmd = Command::SetParagraphFormat {
        slide,
        shape,
        start: 0,
        end: text.chars().count(),
        format: ParagraphFormat {
            list: Some(ListKind::Numbered),
            start_at: Some(5),
            ..Default::default()
        },
    };
    let slide_idx = s.p().slide_index(slide).unwrap() + 1;
    let saved = s.run(vec![cmd]);
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = String::from_utf8(package.read(&format!("ppt/slides/slide{slide_idx}.xml")).unwrap()).unwrap();
    assert!(xml.contains("startAt=\"5\""), "OOXML contains startAt=\"5\": {xml}");

    let reopened = ibis_pptx::read(&saved).unwrap();
    let body = reopened.slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap();
    assert_eq!(body.paragraphs[0].bullet.as_ref().unwrap().text, "5.");
    assert_eq!(body.paragraphs[0].bullet.as_ref().unwrap().auto.as_ref().unwrap().start_at, 5);
    if body.paragraphs.len() > 1 {
        assert_eq!(body.paragraphs[1].bullet.as_ref().unwrap().text, "6.");
    }

    // 2. Change start_at to 10.
    let cmd_update = Command::SetParagraphFormat {
        slide,
        shape,
        start: 0,
        end: text.chars().count(),
        format: ParagraphFormat {
            start_at: Some(10),
            ..Default::default()
        },
    };
    let saved10 = s.run(vec![cmd_update]);
    let package10 = ibis_ooxml::Package::open(saved10.clone()).unwrap();
    let xml10 = String::from_utf8(package10.read(&format!("ppt/slides/slide{slide_idx}.xml")).unwrap()).unwrap();
    assert!(xml10.contains("startAt=\"10\""), "OOXML contains startAt=\"10\": {xml10}");

    let reopened10 = ibis_pptx::read(&saved10).unwrap();
    let body10 = reopened10.slide(slide).unwrap().shape(shape).unwrap().text_body().unwrap();
    assert_eq!(body10.paragraphs[0].bullet.as_ref().unwrap().text, "10.");
    assert_eq!(body10.paragraphs[0].bullet.as_ref().unwrap().auto.as_ref().unwrap().start_at, 10);
    if body10.paragraphs.len() > 1 {
        assert_eq!(body10.paragraphs[1].bullet.as_ref().unwrap().text, "11.");
    }

    // 3. Undo round trip
    assert!(s.document.undo());
    assert!(s.document.undo());
    assert_eq!(s.p(), &original, "undo returns to original state");
}

#[test]
fn slide_sldimg_and_hdr_placeholder_roundtrip() {
    let base = ibis_pptx::blank_file(ibis_pptx::Kind::Presentation).unwrap();
    let mut pkg = ibis_ooxml::Package::open(base).unwrap();
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    let extra_shapes = concat!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="30" name="sldImgPh"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="sldImg" idx="10"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="914400" y="914400"/><a:ext cx="4000000" cy="3000000"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="ja-JP" dirty="0"/><a:t>プレースホルダー sldImg</a:t></a:r></a:p></p:txBody></p:sp>"#,
        r#"<p:sp><p:nvSpPr><p:cNvPr id="31" name="hdrPh"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="hdr" idx="11"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="5000000" y="914400"/><a:ext cx="4000000" cy="3000000"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="ja-JP" dirty="0"/><a:t>プレースホルダー hdr</a:t></a:r></a:p></p:txBody></p:sp>"#,
        r#"<p:sp><p:nvSpPr><p:cNvPr id="32" name="minimalSldImg"/><p:cNvSpPr/><p:nvPr><p:ph type="sldImg" idx="12"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:p/></p:txBody></p:sp>"#,
        r#"<p:sp><p:nvSpPr><p:cNvPr id="33" name="minimalHdr"/><p:cNvSpPr/><p:nvPr><p:ph type="hdr" idx="13"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:p/></p:txBody></p:sp>"#,
    );
    let slide_xml = slide_xml.replacen("</p:spTree>", &format!("{extra_shapes}</p:spTree>"), 1);
    pkg.set_bytes("ppt/slides/slide1.xml", slide_xml.into_bytes());
    let bytes = pkg.save(SaveOptions::default()).unwrap();

    let opened = ibis_pptx::open(bytes).unwrap();
    let slide = &opened.presentation.slides[0];
    let sld_img_id = *slide.placeholders.iter().find(|(_, p)| p.placeholder.kind.as_deref() == Some("sldImg") && p.placeholder.index.as_deref() == Some("10")).expect("sldImg placeholder read").0;
    let hdr_id = *slide.placeholders.iter().find(|(_, p)| p.placeholder.kind.as_deref() == Some("hdr") && p.placeholder.index.as_deref() == Some("11")).expect("hdr placeholder read").0;

    let mut doc = Document::new(opened.presentation.clone());
    doc.execute(vec![Command::EditText {
        slide: slide.id,
        shape: sld_img_id,
        at: 0,
        remove: 0,
        insert: "!".into(),
    }]).unwrap();

    let saved = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    let saved_pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let saved_xml = String::from_utf8(saved_pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(saved_xml.contains(r#"type="sldImg" idx="10""#), "sldImg placeholder preserved: {saved_xml}");
    assert!(saved_xml.contains(r#"type="hdr" idx="11""#), "hdr placeholder preserved: {saved_xml}");
    assert!(saved_xml.contains(r#"type="sldImg" idx="12""#), "minimal sldImg placeholder preserved: {saved_xml}");
    assert!(saved_xml.contains(r#"type="hdr" idx="13""#), "minimal hdr placeholder preserved: {saved_xml}");

    let reread = ibis_pptx::read(&saved).unwrap();
    let r_slide = &reread.slides[0];
    assert_eq!(r_slide.shape(sld_img_id).unwrap().text_body().unwrap().text(), "!プレースホルダー sldImg");
    assert_eq!(r_slide.shape(hdr_id).unwrap().text_body().unwrap().text(), "プレースホルダー hdr");
}



/// Issue #16: an empty `<a:effectLst/>` in a shape's `spPr` turns off the theme's effect its `<p:style>` names (`effectRef`):
/// no shadow, where the same shape without it has the theme's (the fixture's third effect style is an outer shadow).
#[test]
fn an_empty_effect_list_turns_off_the_themes_shadow() {
    let mut s = Scenario::open("empty-effect-list", "editing/pptx-plan.pptx");
    let slide = s.p().slides[0].id;
    let index = s.p().slides[0].shapes.len();
    let id = s.document.ids().new_shape_id();
    let auto = AutoShape::new(Geometry::Rect, Fill::Solid(Color::BLACK), None);
    let saved = s.run(vec![Command::InsertShape { slide, index, shape: Shape::new(id, EmuRect::new(500_000, 500_000, 2_000_000, 1_000_000), ShapeKind::Auto(auto)) }]);
    let with_style = |empty_list: bool| -> Option<ibis_doc_model::Shadow> {
        let mut package = ibis_ooxml::Package::open(saved.clone()).unwrap();
        let xml = package.xml_mut("ppt/slides/slide1.xml").unwrap();
        let root = xml.root_element().unwrap();
        let sp = xml.descendants(root).into_iter().filter(|&n| xml.is_element(n) && xml.local_name(n) == "sp").last().unwrap();
        let sp_pr = xml.child(sp, "spPr").unwrap();
        if empty_list {
            xml.append_element(sp_pr, "a:effectLst");
        }
        let style = xml.append_element(sp, "p:style");
        xml.insert_after(sp_pr, style);
        for (name, idx) in [("a:lnRef", "0"), ("a:fillRef", "0"), ("a:effectRef", "3")] {
            let r = xml.append_element(style, name);
            xml.set_attr(r, "idx", idx);
            let color = xml.append_element(r, "a:schemeClr");
            xml.set_attr(color, "val", "accent1");
        }
        let font = xml.append_element(style, "a:fontRef");
        xml.set_attr(font, "idx", "minor");
        let data = package.save(SaveOptions::default()).unwrap();
        let opened = ibis_pptx::open(data).unwrap();
        match &opened.presentation.slides[0].shapes[index].kind {
            ShapeKind::Auto(a) => a.shadow,
            _ => panic!("auto"),
        }
    };
    assert!(with_style(false).is_some(), "the theme's shadow, named by effectRef");
    assert!(with_style(true).is_none(), "an empty effectLst turns it off");
}
