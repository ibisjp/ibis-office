// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::check_presentation_compatibility;
use ibis_doc_model::{Presentation, ShapeId, ShapeKind};
use ibis_doc_model::{Clip, ClipOptions, EmuRect, MediaFormat, Picture, Shape};

fn with_clip(link: Option<&str>) -> Presentation {
    let mut p = ibis_pptx::read(&ibis_pptx::blank_file(ibis_pptx::Kind::Presentation).unwrap()).unwrap();
    let poster = p.add_media(MediaFormat::Png, vec![0x89, 0x50, 0x4e, 0x47]);
    let file = p.add_media(MediaFormat::Mp4, Vec::new());
    let pic = Picture {
        media: poster, crop: Default::default(), stroke: None, corner: 0, duotone: None, clear: None, luminance: None,
        clip: Some(Clip { media: file, audio: false, options: ClipOptions::default(), link: link.map(str::to_string) }),
        shadow: None, reflection: None, soft_edge: None, alt_text: Default::default(), decorative: false, name: String::new(),
        geometry: None, effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None,
    };
    p.slides[0].shapes.push(Shape::new(ShapeId(900), EmuRect::new(0, 0, 1_000_000, 1_000_000), ShapeKind::Picture(pic)));
    p
}

/// Issue 62: an embedded video or audio plays in PowerPoint 2010 and later: no issue.
#[test]
fn an_embedded_video_is_not_an_issue_and_a_linked_one_is_only_a_minor_one() {
    let embedded = check_presentation_compatibility(&with_clip(None));
    assert!(embedded.significant.is_empty() && embedded.minor.iter().all(|i| i.shape_id != Some(ShapeId(900))));
    let linked = check_presentation_compatibility(&with_clip(Some("file:///movie.mp4")));
    assert!(linked.significant.is_empty());
    assert!(linked.minor.iter().any(|i| i.shape_id == Some(ShapeId(900))));
}
