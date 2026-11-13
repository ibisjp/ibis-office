// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-11 (#M74, Windows check of the crate split): saving while the master view is open is refused before any dialog asks
//! where, and a value typed in the chart's data grid reaches the chart on the slide.
use crate::pptx::ext::*;
use crate::xlsx::ext::*;
use crate::shell::scenario::{button_probe, Scenario};
use ibis_doc_model::ShapeKind;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::m74_20261111_save_is_refused_in_the_master_view", master_view_save),
        ("pptx::m74_20261111_every_export_and_print_is_refused_in_the_master_view", master_view_exports),
        ("pptx::m74_20261111_typed_chart_data_reaches_the_chart", typed_chart_data),
    ]
}

/// ⌘S, 名前を付けて保存, the confirmation's 保存, a copy and an export while the master view is open: the status tells it is
/// refused, no dialog asks a name (the master page has no path of its own) and nothing is written.
fn master_view_save() {
    let mut s = Scenario::open("test/editing/pptx-menu-states.pptx");
    s.menu("pptx.master-view");
    assert!(s.with(|e, _| e.is_master_view()));
    button_probe::catch_save_as(true);
    s.update(|e, w, cx| e.save(&crate::shell::actions::Save, w, cx));
    let asks_save = button_probe::save_as_asks();
    let status = s.with(|e, _| e.status.as_ref().map(|t| t.to_string()).unwrap_or_default());
    s.update(|e, w, cx| e.save_as(&crate::shell::actions::SaveAs, w, cx));
    let asks_as = button_probe::save_as_asks();
    s.update(|e, w, cx| e.save_then(w, cx, |_, _, _| panic!("closed after a refused save")));
    let asks_then = button_probe::save_as_asks();
    button_probe::catch_save_as(false);
    assert_eq!((asks_save, asks_as, asks_then), (0, 0, 0), "no dialog asks where to save");
    assert!(status.contains(&*crate::shell::i18n::text("_shell.close-slide-master-view-and-notes-formatting-bef")), "the refusal is told: {status}");
    let target = std::path::PathBuf::from("target/scenario/m74-copy.pptx");
    assert!(s.with(|e, _| e.copy_to(&target)).is_err(), "a copy is refused too");
    assert!(!target.exists());
    assert!(s.with(|e, _| e.current_bytes()).is_err(), "an export is refused too");
    // Closed, saving is as before.
    s.menu("pptx.master-view");
    assert!(!s.with(|e, _| e.is_master_view()));
    s.update(|e, w, cx| e.save(&crate::shell::actions::Save, w, cx));
    assert!(s.with(|e, _| e.status.as_ref().is_some_and(|t| !t.contains(&*crate::shell::i18n::text("_shell.close-slide-master-view-and-notes-formatting-bef")))));
}

/// Every way out of the file (each export, the copy, printing) while the master view is open: told as refused at once, with no
/// dialog asking where and no print panel, and `bytes_for_path` refuses every extension (a .ppsx made from the master view's
/// page lost the layout's shapes and gained a slide; an .odp held the layout alone).
fn master_view_exports() {
    let mut s = Scenario::open("test/editing/pptx-menu-states.pptx");
    s.menu("pptx.master-view");
    assert!(s.with(|e, _| e.is_master_view()));
    let refusal = crate::shell::i18n::text("_shell.close-slide-master-view-and-notes-formatting-bef").to_string();
    type Way = (&'static str, fn(&mut crate::shell::editor::SlideEditor, &mut gpui_kit::Window, &mut gpui_kit::Context<crate::shell::editor::SlideEditor>));
    let ways: Vec<Way> = vec![
        ("pdf", |e, w, cx| e.export_pdf(w, cx)),
        ("png", |e, w, cx| e.export_png(w, cx)),
        ("current slide png", |e, w, cx| e.export_current_slide_png(w, cx)),
        ("gif", |e, w, cx| e.export_gif(w, cx)),
        ("odp", |e, w, cx| e.export_odp(w, cx)),
        ("ppsx", |e, w, cx| e.export_ppsx(w, cx)),
        ("potx", |e, w, cx| e.export_potx(w, cx)),
        ("thmx", |e, w, cx| e.export_thmx(w, cx)),
        ("copy", |e, w, cx| e.save_copy(w, cx)),
        ("print", |e, w, cx| e.print(w, cx)),
    ];
    for (name, way) in ways {
        s.update(|e, _, _| e.status = None);
        s.update(|e, w, cx| way(e, w, cx));
        let status = s.with(|e, _| e.status.as_ref().map(|t| t.to_string()).unwrap_or_default());
        assert!(status.contains(&refusal), "{name}: the refusal is told at once: {status}");
        assert!(s.with(|e, _| e.pptx().is_some_and(|p| p.print_panel.is_none())), "{name}: no print panel opens");
    }
    // The shared entry for the bytes of any extension.
    for extension in ["odp", "ppsx", "potx", "pptx", "pptm", "pdf"] {
        let path = std::path::PathBuf::from(format!("target/scenario/m74-refuse.{extension}"));
        let made = s.with(|e, _| e.source.clone().map(|source| crate::shell::format::with_format!(e, bytes_for_path(e, &path, &source, true))));
        assert!(matches!(made, Some(Some(Err(_))) | Some(None)), "{extension}: no bytes are made");
        if extension != "pdf" {
            assert!(matches!(made, Some(Some(Err(_)))), "{extension}: refused");
        }
        assert!(!path.exists());
    }
}

/// A value typed in a cell of the chart's data grid (the cell being edited, then Enter) goes into the chart on the slide.
fn typed_chart_data() {
    let mut s = Scenario::open("test/editing/pptx-menu-states.pptx");
    let (page, shape) = s
        .with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Chart(_))).map(|j| (i, j))))
        .expect("a slide with a chart");
    s.page(page + 1);
    s.click_shape(shape);
    s.menu("pptx.chart.data-quick");
    s.update(|e, w, cx| e.open_chart_data(w, cx));
    let first = |s: &mut Scenario| -> Option<f64> {
        s.with(|e, _| match &e.presentation().slides[page].shapes[shape].kind {
            ShapeKind::Chart(c) => c.groups[0].series[0].values[0].map(|n| n.0),
            _ => None,
        })
    };
    let before = first(&mut s);
    assert!(before.is_some(), "the chart has a first value");
    let (view, refs) = s.with(|e, _| {
        let open = e.chart_data_window().expect("the data grid");
        (open.view.clone(), open.refs.clone())
    });
    let (_, range) = ibis_pptx::parse_ref(refs[0].values.as_deref().expect("a range")).expect("a reference");
    s.with_app(|app| {
        view.update(app, |d, cx| {
            let slide = d.current_slide;
            if let Some(state) = d.sheet_state_mut() {
                state.cells.insert(slide, (range.first_column, range.first_row));
            }
            d.begin_cell_edit(crate::shell::edit_mode::EditMode::Enter, false, cx);
            d.cell_edit_mut().expect("typing").text = "28".into();
            d.commit_cell_edit(cx);
        })
    });
    s.settle();
    assert_ne!(before, Some(28.0));
    assert_eq!(first(&mut s), Some(28.0), "the chart on the slide shows the typed value");
    // The same with the keyboard: the grid has the focus, the next value is typed and Enter pressed.
    let second = |s: &mut Scenario| -> Option<f64> {
        s.with(|e, _| match &e.presentation().slides[page].shapes[shape].kind {
            ShapeKind::Chart(c) => c.groups[0].series[0].values.get(1).copied().flatten().map(|n| n.0),
            _ => None,
        })
    };
    let before_second = second(&mut s);
    s.update(|_, w, cx| view.update(cx, |d, cx| {
        let slide = d.current_slide;
        if let Some(state) = d.sheet_state_mut() {
            state.cells.insert(slide, (range.first_column, range.first_row + 1));
        }
        d.focus_handle.focus(w, cx);
    }));
    s.type_text("29");
    s.key("enter");
    assert_ne!(before_second, Some(29.0));
    assert_eq!(second(&mut s), Some(29.0), "typed with the keyboard: the chart shows it");
}
