// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ズーム cards (挿入 > ズーム: スライド ズーム, セクション ズーム, サマリー ズーム) made the way PowerPoint makes them, from what a
//! file PowerPoint for Mac wrote (`test/zoom-sample/office-zoom.pptx`):
//!
//! - A card is a picture of the slide it goes to (PowerPoint's file carries it as a PNG as many points wide as the card is
//!   wide: 240 pixels for a card 3 048 000 EMU wide), with a light-gray outline of 0.25 pt, a link to the slide and a
//!   `ZoomObject` (`Slide::zoom_objects`). Saving writes the `mc:AlternateContent` PowerPoint reads (`ibis-pptx` `save/pptx_zoom.rs`).
//! - The picture is drawn by ibisOffice's own export (`ibisOffice --export png`, a process of its own, as 印刷 and PDF 書き出し do:
//!   the one way every desktop build draws a slide to a picture); where it cannot run (a test, the Web) the picture is a plain
//!   outlined page, which PowerPoint shows until the card is made again.
//! - A スライド ズーム is a quarter of the slide wide, in the middle; its 戻る is off. A セクション ズーム goes to the section's first
//!   slide and returns (PowerPoint omits `returnToParent`, whose default is true).
//! - A サマリー ズーム makes a new section at the front (サマリー セクション) with a new slide of the Title and Content layout, and one
//!   card for each section chosen, in a grid. A section added, removed or moved changes the cards (`refresh_summary_zooms`).

use gpui_kit::*;
use ibis_doc_model::{Color, Command, EmuRect, Picture, Rgb, Section, Shape, ShapeId, ShapeKind, SlideId, SlideLink, Stroke, ZoomObject, ZoomTarget};

use crate::shell::editor::SlideEditor;

/// The card's outline: PowerPoint's `a:ln w="3175"` in `ltGray`.
fn card_stroke() -> Stroke {
    Stroke::solid(Color::opaque(Rgb(0xD3_D3_D3)), 3175)
}

/// The width in pixels PowerPoint gives a card's picture: one for each point of the card's width.
fn thumbnail_width(frame: EmuRect) -> u32 {
    (frame.width / 12_700).clamp(16, 4096) as u32
}

/// A page with PowerPoint's outline, white: what a card shows where the slide cannot be drawn to a picture.
fn blank_page_png(width: u32, height: u32) -> Vec<u8> {
    let mut image = image::RgbaImage::from_pixel(width, height, image::Rgba([255, 255, 255, 255]));
    for x in 0..width {
        image.put_pixel(x, 0, image::Rgba([0xD3, 0xD3, 0xD3, 255]));
        image.put_pixel(x, height - 1, image::Rgba([0xD3, 0xD3, 0xD3, 255]));
    }
    for y in 0..height {
        image.put_pixel(0, y, image::Rgba([0xD3, 0xD3, 0xD3, 255]));
        image.put_pixel(width - 1, y, image::Rgba([0xD3, 0xD3, 0xD3, 255]));
    }
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(image).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).expect("a PNG in memory");
    out
}

/// A GUID for a new zoom object, from the time and a counter (the form `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}` PowerPoint writes).
fn new_guid(seed: u128) -> String {
    let value = (crate::clock::unix_millis() as u128).wrapping_mul(1_000_003).wrapping_add(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let h = format!("{value:032X}");
    format!("{{{}-{}-{}-{}-{}}}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// The grid PowerPoint lays the cards of a summary in, inside the frame: two columns for three cards. Its numbers are those of
/// the file (a card is 0.45 of the frame's height with two rows, the gap 0.03 of it, the cards centred in both ways); for a
/// number of cards the file does not show, the same rule: the rows fill 0.93 of the height, and the cards keep the slide's shape.
fn summary_grid(count: usize, frame: EmuRect, slide_aspect: f64) -> Vec<EmuRect> {
    let cols = (count as f64).sqrt().ceil().max(1.0) as usize;
    let rows = count.div_ceil(cols).max(1);
    let gap = (frame.height as f64 * 0.03).round() as i64;
    let mut card_h = ((frame.height as f64 * 0.93) - gap as f64 * (rows as f64 - 1.0)) / rows as f64;
    let mut card_w = card_h * slide_aspect;
    // Never wider than the frame.
    let room = (frame.width - gap * (cols as i64 - 1)) as f64 / cols as f64;
    if card_w > room {
        card_w = room;
        card_h = card_w / slide_aspect;
    }
    let (card_w, card_h) = (card_w.round() as i64, card_h.round() as i64);
    let grid_w = card_w * cols as i64 + gap * (cols as i64 - 1);
    let grid_h = card_h * rows as i64 + gap * (rows as i64 - 1);
    let (x0, y0) = (frame.x + (frame.width - grid_w) / 2, frame.y + (frame.height - grid_h) / 2);
    (0..count).map(|i| EmuRect::new(x0 + (i % cols) as i64 * (card_w + gap), y0 + (i / cols) as i64 * (card_h + gap), card_w, card_h)).collect()
}

#[ibis_office_macros::editor_ext(ZoomCards)]
impl SlideEditor {
    /// The slide `slide` drawn as a picture `width` pixels wide (`aspect` is its height over its width), as PNG.
    fn slide_thumbnail_png(&self, slide: SlideId, width: u32, aspect: f64) -> Vec<u8> {
        let height = ((width as f64 * aspect).round() as u32).max(1);
        #[cfg(not(target_family = "wasm"))]
        if !crate::shell::file_actions::running_under_test()
            && let Some(png) = self.render_slide_png(slide, width)
        {
            return png;
        }
        let _ = slide;
        blank_page_png(width, height)
    }

    /// The export program run on the file as it is now, for slide `slide`.
    #[cfg(not(target_family = "wasm"))]
    fn render_slide_png(&self, slide: SlideId, width: u32) -> Option<Vec<u8>> {
        let number = self.presentation().slide_index(slide)? + 1;
        let bytes = self.current_bytes().ok()?;
        let dir = std::env::temp_dir().join(format!("ibisoffice-zoom-{}-{}", std::process::id(), crate::clock::unix_millis()));
        let out = dir.join("out");
        std::fs::create_dir_all(&out).ok()?;
        let file = dir.join("zoom.pptx");
        std::fs::write(&file, bytes).ok()?;
        let program = crate::shell::file_actions::export_program().ok()?;
        let status = std::process::Command::new(program)
            .args(["--export", "png", "--width", &width.to_string(), "--slides", &number.to_string(), "--out-dir"])
            .arg(&out)
            .arg(&file)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()?;
        let png = status.success().then(|| std::fs::read_dir(&out).ok()?.filter_map(Result::ok).map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png"))).and_then(|p| std::fs::read(p).ok())).flatten();
        let _ = std::fs::remove_dir_all(&dir);
        png
    }

    /// The commands that put a card of `target` on slide `on` at `frame`: a picture of slide `shows` with the link, the outline
    /// and the object, 戻る as `returns`. The card's shape id.
    fn zoom_card_commands(&mut self, on: SlideId, frame: EmuRect, target: ZoomTarget, shows: SlideId, returns: bool, index: usize, extra: usize) -> (Vec<Command>, ShapeId) {
        // What the card goes to, for the link's tooltip: "3. Title" for a slide, "Section: Title of its first slide" for a section.
        let title = self
            .presentation()
            .slide(shows)
            .and_then(|s| s.shapes.iter().find_map(|sh| sh.text_body().map(|b| b.text().trim().to_string()).filter(|t| !t.is_empty())))
            .unwrap_or_default();
        let tooltip = match &target {
            ZoomTarget::Slide { .. } => format!("{}. {title}", self.presentation().slide_index(shows).map_or(0, |i| i + 1)),
            ZoomTarget::Section { section_id } | ZoomTarget::Summary { section_id, .. } => {
                format!("{}: {title}", self.presentation().sections.iter().find(|s| &s.id == section_id).map(|s| s.name.as_str()).unwrap_or(""))
            }
        };
        let size = self.presentation().size_of(self.presentation().slide(shows).unwrap_or_else(|| self.slide()));
        let aspect = size.height as f64 / size.width.max(1) as f64;
        let png = self.slide_thumbnail_png(shows, thumbnail_width(frame), aspect);
        let media = self.document.ids().add_media(ibis_doc_model::MediaFormat::Png, png.clone());
        self.resources.add_picture(media, ibis_doc_model::MediaFormat::Png, &png);
        let id = self.document.ids().new_shape_id();
        let mut picture = Picture::new(media);
        picture.stroke = Some(card_stroke());
        let shape = Shape::new(id, frame, ShapeKind::Picture(picture));
        let object = ZoomObject { target, id: new_guid(id.0 as u128 + extra as u128), return_to_parent: returns, transition_dur: Some(1000), show_bg: true, cover_image: false };
        let mut commands = vec![
            Command::InsertShape { slide: on, index, shape },
            Command::SetShapeLink { slide: on, shape: id, link: Some(SlideLink::Slide(shows)) },
            Command::SetShapeLinkTooltip { slide: on, shape: id, tooltip: Some(tooltip) },
            Command::SetAspectLocked { slide: on, shape: id, locked: true },
            Command::SetZoomObject { slide: on, shape: id, zoom: Some(object) },
            Command::SetShapeZoomDuration { slide: on, shape: id, duration_ms: Some(1000) },
        ];
        if returns {
            commands.push(Command::SetShapeReturnToZoom { slide: on, shape: id, return_to_zoom: true });
        }
        (commands, id)
    }

    /// The picture of slide `shows` for the card `shape` of the current slide, as new media.
    pub(crate) fn zoom_preview_media(&mut self, shows: SlideId, shape: ShapeId) -> ibis_doc_model::MediaId {
        let frame = self.slide().shape(shape).map_or(EmuRect::new(0, 0, 3_048_000, 1_714_500), |s| s.frame);
        let size = self.presentation().size_of(self.presentation().slide(shows).unwrap_or_else(|| self.slide()));
        let png = self.slide_thumbnail_png(shows, thumbnail_width(frame), size.height as f64 / size.width.max(1) as f64);
        let media = self.document.ids().add_media(ibis_doc_model::MediaFormat::Png, png.clone());
        self.resources.add_picture(media, ibis_doc_model::MediaFormat::Png, &png);
        media
    }

    /// The frame of a new スライド/セクション ズーム: a quarter of the slide wide, in the middle, each next one a step lower and to
    /// the right (so the ones before stay in sight).
    fn zoom_card_frame(&self, nth: usize) -> EmuRect {
        let size = self.presentation().size_of(self.slide());
        let (w, h) = (size.width / 4, size.height / 4);
        let step = (nth as i64 % 5) * 300_000;
        EmuRect::new((size.width - w) / 2 + step, (size.height - h) / 2 + step, w, h)
    }

    /// 挿入 > ズーム > スライド ズーム: a card of `target` on the current slide.
    pub(crate) fn insert_slide_zoom(&mut self, target: SlideId, cx: &mut Context<Self>) -> Option<ShapeId> {
        self.presentation().slide(target)?;
        let nth = self.slide().zoom_objects.len();
        let frame = self.zoom_card_frame(nth);
        let creation = None;
        let index = self.slide().shapes.len();
        let (commands, id) = self.zoom_card_commands(self.current_slide, frame, ZoomTarget::Slide { slide: target, creation_id: creation }, target, false, index, 0);
        self.run(commands, cx);
        self.set_selection(vec![id]);
        Some(id)
    }

    /// 挿入 > ズーム > セクション ズーム: a card of section `section_id` (it goes to the section's first slide).
    pub(crate) fn insert_section_zoom(&mut self, section_id: &str, cx: &mut Context<Self>) -> Option<ShapeId> {
        let first = self.presentation().sections.iter().find(|s| s.id == section_id)?.slides.first().copied()?;
        let nth = self.slide().zoom_objects.len();
        let frame = self.zoom_card_frame(nth);
        let index = self.slide().shapes.len();
        let (commands, id) = self.zoom_card_commands(self.current_slide, frame, ZoomTarget::Section { section_id: section_id.to_string() }, first, true, index, 0);
        self.run(commands, cx);
        self.set_selection(vec![id]);
        Some(id)
    }

    /// 挿入 > ズーム > サマリー ズーム for the sections `section_ids`: a new section at the front, サマリー セクション, with a new
    /// slide of the Title and Content layout and a card for each. The cards' shape ids.
    pub(crate) fn insert_summary_zoom_for_sections(&mut self, section_ids: &[String], window: &mut Window, cx: &mut Context<Self>) -> Vec<ShapeId> {
        let sections: Vec<Section> = self.presentation().sections.clone();
        let chosen: Vec<&Section> = section_ids.iter().filter_map(|id| sections.iter().find(|s| &s.id == id)).filter(|s| !s.slides.is_empty()).collect();
        if chosen.is_empty() {
            return Vec::new();
        }
        let layout = {
            let p = self.presentation();
            p.layouts.iter().find(|l| l.name == "タイトルとコンテンツ" || l.name.eq_ignore_ascii_case("Title and Content")).or_else(|| p.layouts.get(1)).or_else(|| p.layouts.first()).map(|l| l.id) // i18n-ok（試験・ファイルの中の日本語のデータ）
        };
        let slide_id = self.document.ids().new_slide_id();
        let mut slide = self.new_slide(0, slide_id, layout);
        let size = self.presentation().size_of(&slide);
        // The frame is the content placeholder's: the layout's, as PowerPoint puts the summary in it.
        let frame = slide.shapes.iter().find(|s| slide.placeholders.get(&s.id).is_some_and(|p| p.placeholder.normalized() != "title")).map(|s| s.frame).unwrap_or_else(|| EmuRect::new(size.width / 14, size.height * 266 / 1000, size.width * 86 / 100, size.height * 634 / 1000));
        // The summary takes the place of the content placeholder: the slide keeps its title only.
        let content: Vec<ShapeId> = slide.shapes.iter().filter(|s| slide.placeholders.get(&s.id).is_some_and(|p| p.placeholder.normalized() != "title")).map(|s| s.id).collect();
        slide.shapes.retain(|s| !content.contains(&s.id));
        for id in &content {
            slide.placeholders.remove(id);
        }
        let grid = summary_grid(chosen.len(), frame, size.width as f64 / size.height.max(1) as f64);
        // The old sections come after the new one; the summary's slide is the only one in it.
        let summary_section = Section { id: new_guid(slide_id.0 as u128 ^ 0x5A5A), name: crate::shell::i18n::text("pptx.zoom.summary-section").to_string(), slides: vec![slide_id] };
        let mut new_sections = vec![summary_section];
        new_sections.extend(sections.iter().cloned());
        let mut commands = vec![Command::InsertSlide { index: 0, slide }];
        let mut cards = Vec::new();
        // The thumbnails are of the slides as they are now; the cards go on the new slide, which does not exist until the
        // command runs: they are put on it after.
        let mut prepared = Vec::new();
        for (i, section) in chosen.iter().enumerate() {
            prepared.push((section.id.clone(), section.slides[0], grid[i]));
        }
        commands.push(Command::SetSections { sections: new_sections });
        self.run(commands, cx);
        let mut card_commands = Vec::new();
        for (i, (section_id, first, frame)) in prepared.into_iter().enumerate() {
            let index = self.presentation().slide(slide_id).map_or(0, |s| s.shapes.len() + i);
            let (c, id) = self.zoom_card_commands(slide_id, frame, ZoomTarget::Summary { section_id, index: i }, first, true, index, i);
            card_commands.extend(c);
            cards.push(id);
        }
        self.run(card_commands, cx);
        self.select_slide(slide_id, window, cx);
        self.set_selection(cards.clone());
        cards
    }

    /// The summary slides' cards after the sections or the slides' order changed: a card of a section that is gone is taken
    /// away; one whose section's first slide changed goes to that slide (and shows its picture).
    pub(crate) fn refresh_summary_zooms(&mut self, cx: &mut Context<Self>) {
        let sections = self.presentation().sections.clone();
        let mut commands = Vec::new();
        let mut relink: Vec<(SlideId, ShapeId, SlideId)> = Vec::new();
        for slide in &self.presentation().slides {
            for (shape, zoom) in &slide.zoom_objects {
                let section_id = match &zoom.target {
                    ZoomTarget::Summary { section_id, .. } | ZoomTarget::Section { section_id } => section_id,
                    ZoomTarget::Slide { .. } => continue,
                };
                let first = sections.iter().find(|s| &s.id == section_id).and_then(|s| s.slides.first().copied());
                match first {
                    None => {
                        commands.push(Command::SetZoomObject { slide: slide.id, shape: *shape, zoom: None });
                        commands.push(Command::RemoveShape { slide: slide.id, shape: *shape });
                    }
                    Some(first) => {
                        if slide.shape_links.get(shape) != Some(&SlideLink::Slide(first)) {
                            relink.push((slide.id, *shape, first));
                        }
                    }
                }
            }
        }
        for (on, shape, first) in relink {
            let Some(frame) = self.presentation().slide(on).and_then(|s| s.shape(shape)).map(|s| s.frame) else { continue };
            let size = self.presentation().size_of(self.presentation().slide(first).unwrap_or_else(|| self.slide()));
            let png = self.slide_thumbnail_png(first, thumbnail_width(frame), size.height as f64 / size.width.max(1) as f64);
            let media = self.document.ids().add_media(ibis_doc_model::MediaFormat::Png, png.clone());
            self.resources.add_picture(media, ibis_doc_model::MediaFormat::Png, &png);
            let Some(ShapeKind::Picture(old)) = self.presentation().slide(on).and_then(|s| s.shape(shape)).map(|s| s.kind.clone()) else { continue };
            let mut picture = old;
            picture.media = media;
            commands.push(Command::SetKind { slide: on, shape, kind: Box::new(ShapeKind::Picture(picture)) });
            commands.push(Command::SetShapeLink { slide: on, shape, link: Some(SlideLink::Slide(first)) });
        }
        // Order the removals so a shape id is removed once.
        if !commands.is_empty() {
            self.run(commands, cx);
        }
    }
}

/// The grid of PowerPoint's own file, for the scenario `zoom_20261105_summary_grid_is_the_files`.
#[cfg(feature = "scenarios")]
pub(crate) fn grid_for_test(count: usize, frame: EmuRect, aspect: f64) -> Vec<EmuRect> {
    summary_grid(count, frame, aspect)
}
