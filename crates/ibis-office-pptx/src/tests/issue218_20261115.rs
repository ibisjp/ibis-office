// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-15 (#218): a shape that waits for its entrance effect is not shown while a 変形 into its slide plays.
use crate::shell::scenario::Scenario;
use crate::ext::*;
use ibis_doc_model::{Color, Command, Fill, Rgb, Transition};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("pptx::issue218_morph_does_not_show_shapes_waiting_for_their_entrance_effect", morph_hides_waiting_shapes)]
}

fn morph_hides_waiting_shapes() {
    let green_in_mid = |animated: bool| {
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        s.size(960., 540.);
        let second = s.with(|e, _| e.presentation().slides[1].id);
        let green = Fill::Solid(Color::opaque(Rgb::from_rgb(0, 255, 0)));
        let shapes: Vec<_> = s.with(|e, _| e.presentation().slides[1].shapes.iter().map(|sh| sh.id).collect());
        s.update(|e, _, cx| {
            let mut cmds: Vec<Command> = shapes.iter().map(|&shape| Command::SetFill { slide: second, shape, fill: green.clone() }).collect();
            cmds.push(Command::SetTransition { slide: second, transition: Some(Transition { duration: 2000, ..Transition::new("morph") }) });
            e.run(cmds, cx)
        });
        s.page(2);
        if animated {
            s.select_shape(0);
            s.toolbar_menu("motion", "pptx.animation.preset.blinds");
            assert!(s.with(|e, _| !e.slide().animations.is_empty()));
            let waiting = s.with(|e, _| crate::slideshow::hidden_before_animation(e.slide()).len());
            assert_eq!(waiting, 1, "the shape with an entrance effect is hidden before it");
        }
        s.page(1);
        s.update(|e, w, cx| e.start_slideshow(w, cx));
        s.settle();
        s.key("space");
        s.update(|e, _, cx| {
            if let Some(show) = e.slideshow_mut() {
                show.transition_at = Some(1000.0);
            }
            cx.notify();
        });
        s.settle();
        let shot = image::open(s.screenshot(if animated { "morph-waiting" } else { "morph-plain" })).unwrap().to_rgba8();
        shot.pixels().filter(|p| p[1] as i32 > p[0] as i32 + 60 && p[1] as i32 > p[2] as i32 + 60).count()
    };
    let plain = green_in_mid(false);
    let waiting = green_in_mid(true);
    assert!(plain > 0, "the control shows the green shape ({plain})");
    assert!(waiting < plain, "the shape waiting for its effect is not shown by the morph ({waiting} of {plain} green pixels)");
}

