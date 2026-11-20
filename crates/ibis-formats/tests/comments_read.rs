// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! コメント read from files PowerPoint saved (test/editing/
//! pptx-comments-modern.pptx: threaded comments; -legacy.pptx: the older
//! ones), with their authors, answers, resolved state and shape.

fn read(name: &str) -> ibis_doc_model::Presentation {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(name);
    ibis_formats::read(&std::fs::read(path).unwrap()).unwrap()
}

/// (author, text, resolved, answers as (author, text)) of a slide's comments.
fn summary(slide: &ibis_doc_model::Slide) -> Vec<(String, String, bool, Vec<(String, String)>)> {
    slide.comments.iter().map(|c| (c.author.name.clone(), c.text.clone(), c.resolved, c.replies.iter().map(|r| (r.author.name.clone(), r.text.clone())).collect())).collect()
}

fn s(v: &str) -> String {
    v.to_string()
}

#[test]
fn threaded_comments_are_read() {
    let p = read("pptx-comments-modern.pptx");
    assert_eq!(
        summary(&p.slides[0]),
        vec![
            (s("山田 花子"), s("コメント1"), false, vec![(s("Taro Test"), s("返信1"))]),
            (s("Taro Test"), s("コメント2"), true, vec![]),
        ]
    );
    // コメント2 is on the title.
    assert_eq!(p.slides[0].comments[1].shape, Some(p.slides[0].shapes[0].id));
    assert_eq!(p.slides[0].comments[0].shape, None);
    assert_eq!(p.slides[0].comments[0].created, "2026-10-07T01:00:00");
    assert_eq!(summary(&p.slides[1]), vec![(s("山田 花子"), s("コメント3"), false, vec![])]);
}

#[test]
fn older_comments_are_read_with_their_answers() {
    let p = read("pptx-comments-legacy.pptx");
    assert_eq!(
        summary(&p.slides[0]),
        vec![
            (s("山田 花子"), s("コメント1"), false, vec![(s("Taro Test"), s("返信1"))]),
            (s("Taro Test"), s("コメント2"), false, vec![]),
        ]
    );
    assert_eq!(summary(&p.slides[1]), vec![(s("山田 花子"), s("コメント3"), false, vec![])]);
}

#[test]
fn comments_stay_when_saved_untouched() {
    for name in ["pptx-comments-modern.pptx", "pptx-comments-legacy.pptx"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(name);
        let data = std::fs::read(path).unwrap();
        let opened = ibis_formats::open(data.clone()).unwrap();
        let saved = opened.source.save(&opened.presentation, ibis_formats::SaveOptions::default()).unwrap();
        assert_eq!(saved, data, "{name}: untouched, the same file");
    }
}
