// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::{Scenario, button_probe as buttons};
use crate::docx::scenarios::DocSteps;
use crate::shell::{chrome::MenuEntry, links::CAPTURE_URLS};
use gpui_kit::*;
use ibis_doc_model::{Block, FlowPos, Link};
use crate::pptx::ext::*;
use crate::docx::ext::*;
use crate::xlsx::ext::*;
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::hyperlink_docx_apply_and_follow", docx),
        ("shell::hyperlink_docx_short_body_link", docx_short),
        ("shell::hyperlink_pptx_autolink_appearance", pptx_auto),
        ("shell::hyperlink_xlsx_apply_and_follow", xlsx),
        ("shell::hyperlink_xlsx_only_the_drawn_text_opens", xlsx_text_area),
        ("shell::hyperlink_xlsx_click_matrix", xlsx_click_matrix),
        ("shell::hyperlink_xlsx_number_cell_frozen_row", xlsx_number_cell),
        ("shell::hyperlink_pptx_apply_and_follow", pptx),
        ("shell::hyperlink_pptx_distinct_text_targets", pptx_text),
        ("shell::hyperlink_pptx_distinct_cell_targets", pptx_cell),
        ("shell::hyperlink_docx_multi_paragraph", docx_multi),
    ]
}
struct UrlCapture;
impl UrlCapture {
    fn new() -> Self {
        CAPTURE_URLS.with(|s| *s.borrow_mut() = Some(Vec::new()));
        Self
    }
    fn count() -> usize {
        CAPTURE_URLS.with(|s| s.borrow().as_ref().unwrap().len())
    }
    fn last() -> String {
        CAPTURE_URLS.with(|s| {
            s.borrow()
                .as_ref()
                .unwrap()
                .last()
                .cloned()
                .unwrap_or_default()
        })
    }
}
impl Drop for UrlCapture {
    fn drop(&mut self) {
        CAPTURE_URLS.with(|s| *s.borrow_mut() = None);
    }
}
fn submenu(entries: &[MenuEntry]) -> &[MenuEntry] {
    entries
        .iter()
        .find_map(|e| match e {
            MenuEntry::Submenu(s) if s.id == "hyperlink.menu" => Some(s.items.as_slice()),
            _ => None,
        })
        .expect("Hyperlink submenu")
}
fn run_menu(s: &mut Scenario, items: &[MenuEntry], id: &str) {
    let f = items
        .iter()
        .find_map(|i| match i {
            MenuEntry::Item(i) if i.id == id => i.handler.clone(),
            _ => None,
        })
        .expect("menu command");
    s.update(|e, w, cx| {
        e.link_popup = None;
        f(e, w, cx);
    });
}
fn docx() {
    let _probe = buttons::Probe::new();
    let _urls = UrlCapture::new();
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let para = s
        .document()
        .body
        .iter()
        .find_map(|b| {
            if let Block::Paragraph(p) = b {
                Some(p.id)
            } else {
                None
            }
        })
        .unwrap();
    s.update(|e, _, cx| e.set_caret(FlowPos::new(para, 0), false, cx));
    let before = s.document();
    s.update(|e, w, cx| e.open_link(w, cx));
    assert!(
        buttons::bounds("link-remove").is_none(),
        "new link has no Remove button"
    );
    s.update(|e, w, cx| {
        let input = e
            .doc_state()
            .unwrap()
            .link
            .as_ref()
            .unwrap()
            .address
            .clone();
        input.update(cx, |i, cx| i.set_value("https://example.com/new", w, cx));
    });
    buttons::click(&mut s, "link-apply");
    assert!(s.with(|e, _| e.doc_state().unwrap().link.is_none()));
    assert!(s.document() != before);
    assert!(s.with(|e, _| {
        e.doc_link_range(FlowPos::new(para, 0))
            .is_some_and(|(_, _, l)| l == Link::Url("https://example.com/new".into()))
    }));
    s.key("cmd-z");
    assert!(
        s.document() == before,
        "URL insertion and its link are one undo"
    );
    s.key("cmd-shift-z");
    // Actual pointer events: hover and ordinary click (no command modifier).
    let at = s.update(|e, _, _| {
        let lines = e.flow_lines();
        let (page, x, y, h) = e.caret_box(&lines, FlowPos::new(para, 1)).unwrap();
        let (o, k) = e.page_origin(page).unwrap();
        (
            f32::from(o.x) + x * k + 1.,
            f32::from(o.y) + (y + h / 2.) * k,
        )
    });
    s.hover(at);
    assert!(s.with(|e, _| e.link_hover));
    s.click(at);
    assert_eq!(UrlCapture::last(), "https://example.com/new");
    s.right_click(at);
    assert!(s.with(|e, _| e.link_popup.is_some()));
    let entries = s.with(|e, _| e.doc_hyperlink_menu());
    let items = submenu(&entries);
    assert_eq!(items.len(), 3);
    run_menu(&mut s, items, "hyperlink.edit");
    assert!(buttons::bounds("link-remove").is_some());
    s.update(|e, w, cx| {
        let input = e
            .doc_state()
            .unwrap()
            .link
            .as_ref()
            .unwrap()
            .address
            .clone();
        input.update(cx, |i, cx| i.set_value("https://example.com/edited", w, cx));
    });
    buttons::click(&mut s, "link-apply");
    assert!(s.with(|e, _| {
        e.doc_link_range(FlowPos::new(para, 0))
            .is_some_and(|(_, _, l)| l == Link::Url("https://example.com/edited".into()))
    }));
    let entries = s.with(|e, _| e.doc_hyperlink_menu());
    run_menu(&mut s, submenu(&entries), "hyperlink.remove");
    assert!(s.with(|e, _| e.doc_link_range(FlowPos::new(para, 0)).is_none()));
}
fn xlsx() {
    use crate::xlsx::scenarios::SheetSteps;
    let _probe = buttons::Probe::new();
    let _urls = UrlCapture::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("B8");
    s.enter("Go", "enter");
    s.click_cell("B8");
    s.update(|e, w, cx| e.open_link_panel(w, cx));
    assert!(buttons::bounds("link-remove").is_none());
    s.update(|e, w, cx| {
        let input = e
            .sheet_state()
            .unwrap()
            .link
            .as_ref()
            .unwrap()
            .address
            .clone();
        input.update(cx, |i, cx| i.set_value("https://example.com/sheet", w, cx));
    });
    buttons::click(&mut s, "link-insert");
    assert!(s.with(|e, _| e.current_link().is_some()));
    s.update(|e, w, cx| e.close_dialog(w, cx));
    s.click_cell("E3");
    let centre = s.with(|e, _| e.cell_center(1, 7).unwrap());
    s.hover((f32::from(centre.x), f32::from(centre.y)));
    // The link opens on the text drawn in the cell.
    let (l, t, r, b) = s.with(|e, _| e.link_text_window_box(1, 7)).expect("the link text is drawn");
    let at = ((l + r) / 2., (t + b) / 2.);
    s.hover(at);
    assert!(s.with(|e, _| e.link_hover));
    s.click(at);
    assert_eq!(UrlCapture::last(), "https://example.com/sheet");
    let entries = s.with(|e, _| e.cell_menu());
    let items = submenu(&entries);
    assert_eq!(items.len(), 3);
    run_menu(&mut s, items, "xlsx.cell-link-remove");
    assert!(s.with(|e, _| e.current_link().is_none()));
}
/// A link opens only where its text is drawn (left, centre and right aligned); elsewhere in the cell the click selects it,
/// and the pointer is the hand over the text only.
fn cell_at(s: &mut Scenario, x: f32, y: f32) -> Option<(u32, u32)> {
    s.with(|e, _| e.cell_under(gpui::point(gpui::px(x), gpui::px(y))))
}
fn xlsx_text_area() {
    use crate::xlsx::scenarios::SheetSteps;
    let _probe = buttons::Probe::new();
    let _urls = UrlCapture::new();
    use ibis_doc_model::{CellAlign as A, CellVerticalAlign as V, StyleChange as C};
    // (formats, text, merged over B..D and two rows)
    let cases: Vec<(Vec<C>, &str, bool)> = vec![
        (vec![C::Align(A::Left)], "Go", false),
        (vec![C::Align(A::Center), C::Vertical(V::Top)], "Go", false),
        (vec![C::Align(A::Right), C::Vertical(V::Bottom)], "Go", false),
        (vec![C::Align(A::Center), C::Rotation(90)], "Go", false),
        (vec![C::Align(A::Left), C::Rotation(45)], "Go", false),
        (vec![C::Rotation(255)], "Go", false),
        (vec![C::Wrap(true), C::Vertical(V::Center)], "Go Go Go Go Go", false),
        (vec![C::Align(A::Center), C::Vertical(V::Center)], "Go", true),
        (vec![C::Align(A::Right), C::Vertical(V::Bottom)], "Go", true),
    ];
    for (k, (changes, text, merged)) in cases.into_iter().enumerate() {
        // A fresh workbook for each: the rows of a screenful only are painted.
        let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
        let row = 7;
        let reference = format!("B{}", row + 1);
        let url = format!("https://example.com/cell{k}");
        s.click_cell(&reference);
        s.enter(text, "enter");
        s.click_cell(&reference);
        if merged {
            let range = ibis_doc_model::CellRange { first_row: row, last_row: row + 1, first_column: 1, last_column: 3 };
            s.update(|e, _, cx| {
                let slide = e.current_slide;
                e.document.execute(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::Merge { slide, range, center: false })]).unwrap();
                cx.notify();
            });
            s.click_cell(&reference);
        }
        for change in changes {
            s.update(|e, _, cx| e.format_cells(change.clone(), cx));
        }
        s.update(|e, w, cx| e.open_link_panel(w, cx));
        s.update(|e, w, cx| {
            let input = e.sheet_state().unwrap().link.as_ref().unwrap().address.clone();
            input.update(cx, |i, cx| i.set_value(&url, w, cx));
        });
        buttons::click(&mut s, "link-insert");
        s.update(|e, w, cx| e.close_dialog(w, cx));
        // Away from it: selected elsewhere, and a paint that records where the text is.
        s.click_cell("E3");
        let centre = s.with(|e, _| e.cell_center(1, row).unwrap());
        s.hover((f32::from(centre.x), f32::from(centre.y)));
        let (l, t, r, b) = s.with(|e, _| e.link_text_window_box(1, row)).unwrap_or_else(|| panic!("{reference} (case {k}): the link text is drawn"));
        assert!(r > l && b > t, "{reference} (case {k}): the text has a box");
        let inside = ((l + r) / 2., (t + b) / 2.);
        // A point in the same cell, outside the text: the cell's far side from the text.
        let spots = [(l - 6., inside.1), (r + 6., inside.1), (inside.0, t - 3.), (inside.0, b + 3.)];
        let blank = spots
            .into_iter()
            .find(|&(x, y)| cell_at(&mut s, x, y).is_some_and(|(c, r)| (1..=3).contains(&c) && (row..=row + 1).contains(&r) && (merged || (c, r) == (1, row))))
            .unwrap_or_else(|| panic!("{reference} (case {k}): a blank point in the cell"));
        s.hover(inside);
        assert!(s.with(|e, _| e.link_hover), "{reference} (case {k}): the hand over the text");
        s.hover(blank);
        assert!(!s.with(|e, _| e.link_hover), "{reference} (case {k}): no hand beside the text, in the cell");
        // The click beside the text selects the cell and opens nothing.
        let before = UrlCapture::count();
        s.click(blank);
        assert_eq!(UrlCapture::count(), before, "{reference} (case {k}): blank part of the cell opens nothing");
        assert_eq!(s.selected(), reference);
        // Another cell: no hand, nothing opens.
        s.click_cell("E3");
        let other = s.with(|e, _| e.cell_center(6, row).unwrap());
        s.hover((f32::from(other.x), f32::from(other.y)));
        assert!(!s.with(|e, _| e.link_hover));
        s.click((f32::from(other.x), f32::from(other.y)));
        assert_eq!(UrlCapture::count(), before);
        // The text: opens the link.
        s.click(inside);
        assert_eq!(UrlCapture::last(), url, "{reference} (case {k}): the text opens the link");
        assert_eq!(UrlCapture::count(), before + 1);
        assert_eq!(s.selected(), reference);
    }
}
/// The text of a link cell under the real pointer, by alignment (4) x zoom (2) x scrolled or not x frozen panes or not: the
/// centre of the drawn text opens the link and shows the hand, a point of the same cell off the text does neither. The
/// cells are in the scrolled part; with frozen panes also in the frozen column and the frozen row.
fn xlsx_click_matrix() {
    use crate::xlsx::scenarios::SheetSteps;
    let _probe = buttons::Probe::new();
    let _urls = UrlCapture::new();
    use ibis_doc_model::{CellAlign as A, StyleChange as C};
    let aligns = [A::General, A::Left, A::Center, A::Right];
    for zoom in [1.0f32, 1.5] {
        for scrolled in [false, true] {
            for frozen in [false, true] {
                let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
                s.update(|e, w, cx| e.set_zoom(Some(zoom), w, cx));
                // (reference, in the frozen panes: column B and row 2 of frozen A1:B2... see below)
                let mut targets: Vec<String> = Vec::new();
                for (k, align) in aligns.iter().enumerate() {
                    let mut refs = vec![format!("E{}", 10 + k)];
                    if frozen {
                        refs.push(format!("C{}", 10 + k));
                        refs.push(format!("{}2", ["F", "G", "H", "I"][k]));
                    }
                    for reference in refs {
                        let url = format!("https://example.com/{reference}-{zoom}-{scrolled}-{frozen}");
                        s.click_cell(&reference);
                        s.enter("Link text", "enter");
                        s.click_cell(&reference);
                        s.update(|e, _, cx| e.format_cells(C::Align(*align), cx));
                        assert_eq!(s.selected(), reference);
                        s.update(|e, _, cx| e.insert_cell_link(&url, cx));
                        targets.push(reference);
                    }
                }
                if frozen {
                    s.click_cell("D3");
                    s.update(|e, _, cx| e.freeze_panes(cx));
                }
                s.click_cell("E5");
                {
                    let ws = s.with(|e, _| e.workspace_bounds).expect("the workspace");
                    let at = (f32::from(ws.origin.x + ws.size.width / 2.), f32::from(ws.origin.y + ws.size.height / 2.));
                    // To the top left first (zooming and selecting scroll the view), then by a little.
                    s.update(|e, _, cx| { e.workspace_scroll.set_offset(gpui::point(gpui::px(0.), gpui::px(0.))); cx.notify(); });
                    s.hover(at);
                    if scrolled {
                    s.wheel(at, -70.);
                    s.wheel_xy(at, -45., 0.);
                    }
                }
                for reference in targets {
                    let (column, row) = {
                        let letters: String = reference.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
                        let number: u32 = reference[letters.len()..].parse().unwrap();
                        (letters.bytes().fold(0, |n, b| n * 26 + (b - b'A' + 1) as u32) - 1, number - 1)
                    };
                    let case = format!("{reference} zoom {zoom} scrolled {scrolled} frozen {frozen}");
                    let url = format!("https://example.com/{reference}-{zoom}-{scrolled}-{frozen}");
                    // Draw (the paint records where the text is), then ask where.
                    let Some(centre) = s.with(|e, _| e.cell_center(column, row)) else { panic!("{case}: no cell centre") };
                    s.hover((f32::from(centre.x), f32::from(centre.y)));
                    let Some((l, t, r, b)) = s.with(|e, _| e.link_text_window_box(column, row)) else {
                        eprintln!("{case}: not drawn (scrolled out of view); skipped");
                        continue;
                    };
                    let inside = ((l + r) / 2., (t + b) / 2.);
                    assert_eq!(cell_at(&mut s, inside.0, inside.1), Some((column, row)), "{case}: the text centre is in the cell");
                    let blank = [(l - 6., inside.1), (r + 6., inside.1), (inside.0, t - 3.), (inside.0, b + 3.)]
                        .into_iter()
                        .find(|&(x, y)| cell_at(&mut s, x, y) == Some((column, row)));
                    s.hover(inside);
                    assert!(s.with(|e, _| e.link_hover), "{case}: the hand over the text");
                    if let Some(blank) = blank {
                        s.hover(blank);
                        assert!(!s.with(|e, _| e.link_hover), "{case}: no hand off the text");
                        let before = UrlCapture::count();
                        s.click(blank);
                        assert_eq!(UrlCapture::count(), before, "{case}: off the text opens nothing");
                    }
                    let before = UrlCapture::count();
                    s.click(inside);
                    assert_eq!(UrlCapture::count(), before + 1, "{case}: the text centre opens the link");
                    assert_eq!(UrlCapture::last(), url, "{case}");
                }
            }
        }
    }
}
/// The user's file (a number cell with an external link, top aligned, wrapped, below a frozen first row): the first link of the
/// sheet, whichever cell the file has it in now. The file is edited by the user, so the link is found, not named.
fn xlsx_number_cell() {
    use crate::xlsx::scenarios::SheetSteps;
    let _probe = buttons::Probe::new();
    let _urls = UrlCapture::new();
    let mut s = Scenario::open("tasks/doc/issue-triage/unlabeled-issues-20261031.xlsx");
    s.click_cell("E5");
    let (column, row, url) = s.with(|e, _| {
        let h = &e.current_sheet().unwrap().1.hyperlinks[0];
        (h.range.first_column, h.range.first_row, h.target.clone().unwrap())
    });
    let centre = s.with(|e, _| e.cell_center(column, row).unwrap());
    s.hover((f32::from(centre.x), f32::from(centre.y)));
    let (l, t, r, b) = s.with(|e, _| e.link_text_window_box(column, row)).expect("the link text is drawn");
    let inside = ((l + r) / 2., (t + b) / 2.);
    assert_eq!(cell_at(&mut s, inside.0, inside.1), Some((column, row)), "the text centre {inside:?} is in the cell (box {:?})", (l, t, r, b));
    s.hover(inside);
    assert!(s.with(|e, _| e.link_hover), "the hand over the text (box {:?})", (l, t, r, b));
    s.click(inside);
    assert_eq!(UrlCapture::last(), url);
}
fn pptx() {
    let _probe = buttons::Probe::new();
    let _urls = UrlCapture::new();
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.click_shape(0);
    s.update(|e, w, cx| e.slide_open_link_panel(w, cx));
    assert!(buttons::bounds("link-remove").is_none());
    s.update(|e, w, cx| {
        let input = e.pptx().unwrap().link_panel.as_ref().unwrap().url.clone();
        input.update(cx, |i, cx| i.set_value("https://example.com/slides", w, cx));
    });
    buttons::click(&mut s, "link-apply");
    let at = s.shape_position(0);
    s.hover(at);
    assert!(s.with(|e, _| e.link_hover));
    s.click(at);
    assert_eq!(UrlCapture::last(), "https://example.com/slides");
    let entries = s.with(|e, _| e.shape_context_menu_entries());
    let items = submenu(&entries);
    assert_eq!(items.len(), 3);
    run_menu(&mut s, items, "hyperlink.remove");
    assert!(
        s.with(|e, _| crate::pptx::links::link_of_shape(e.slide(), e.selected_ids()[0]).is_none())
    );
}

fn docx_multi() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let ids = s
        .document()
        .body
        .iter()
        .filter_map(|b| {
            if let Block::Paragraph(p) = b {
                (p.len() > 2).then_some(p.id)
            } else {
                None
            }
        })
        .take(2)
        .collect::<Vec<_>>();
    s.update(|e, _, cx| {
        e.set_caret(FlowPos::new(ids[0], 1), false, cx);
        e.set_caret(FlowPos::new(ids[1], 2), true, cx);
    });
    let before = s.document();
    s.update(|e, w, cx| {
        e.open_link(w, cx);
        let input = e
            .doc_state()
            .unwrap()
            .link
            .as_ref()
            .unwrap()
            .address
            .clone();
        input.update(cx, |i, cx| i.set_value("https://example.com/range", w, cx));
    });
    buttons::click(&mut s, "link-apply");
    assert!(s.with(|e, _| e.doc_state().unwrap().link.is_none()));
    assert!(s.with(|e, _| {
        ids.iter()
            .all(|id| e.doc_link_range(FlowPos::new(*id, 1)).is_some())
    }));
    s.key("cmd-z");
    assert_eq!(s.document(), before);
}
fn pptx_text() {
    pptx_distinct(false)
}
fn pptx_cell() {
    pptx_distinct(true)
}
fn pptx_distinct(cell_mode: bool) {
    use ibis_doc_model::{Command, SlideLink, TextBody, TextStyle};
    let _probe = buttons::Probe::new();
    let _urls = UrlCapture::new();
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    if cell_mode {
        s.update(|e, w, cx| e.insert_slide_table_with(1, 1, Some(&[vec!["Target".into()]]), w, cx));
    }
    let shape = s.with(|e, _| {
        if cell_mode {
            e.slide()
                .shapes
                .iter()
                .find(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Table(_)))
                .unwrap()
                .id
        } else {
            e.slide().shapes[0].id
        }
    });
    s.update(|e, _, cx| {
        let mut body = TextBody::plain("One", TextStyle::default());
        let first = body.paragraphs[0].runs[0].clone();
        body.paragraphs[0].runs = vec![first.clone(), first.clone(), first];
        for (r, (text, url)) in body.paragraphs[0].runs.iter_mut().zip([
            ("One", Some("https://example.com/one")),
            (" gap ", None),
            ("Two", Some("https://example.com/two")),
        ]) {
            r.text = text.into();
            r.style.link = url.map(|s| SlideLink::Url(s.into()));
        }
        let command = Command::SetTextBody {
            slide: e.current_slide,
            shape,
            body,
        };
        e.run(
            vec![if cell_mode {
                Command::InCell {
                    row: 0,
                    column: 0,
                    command: Box::new(command),
                }
            } else {
                command
            }],
            cx,
        );
    });
    let at = |s: &mut Scenario, index: usize| {
        s.with(|e, _| {
            let cache = e.link_text_positions.borrow();
            let slots = cache
                .get(&(shape, cell_mode.then_some((0, 0))))
                .unwrap()
                .paragraphs
                .borrow();
            let q = slots[0].borrow();
            let c = q.chars[index];
            let line = q.lines[c.line];
            q.to_window(c.x + c.advance / 2., line.top + line.height / 2.)
        })
    };
    let p = at(&mut s, 0);
    s.hover(p);
    assert!(s.with(|e, _| e.link_hover));
    s.click(p);
    assert_eq!(UrlCapture::last(), "https://example.com/one");
    let p = at(&mut s, 4);
    s.hover(p);
    assert!(
        !s.with(|e, _| e.link_hover),
        "unlinked characters retain their normal pointer"
    );
    let p = at(&mut s, 9);
    s.hover(p);
    s.click(p);
    assert_eq!(UrlCapture::last(), "https://example.com/two");
    s.right_click(p);
    assert!(s.with(|e, _| e.link_popup.is_some()));
    let (target, link) = s.with(|e, _| {
        e.slide_link_target_at(e.slide_layout().unwrap().to_emu(point(px(p.0), px(p.1))))
            .unwrap()
    });
    let entry = s.with(|e, _| e.slide_hyperlink_menu_for(target, link));
    let items = submenu(std::slice::from_ref(&entry));
    run_menu(&mut s, items, "hyperlink.edit");
    assert!(buttons::bounds("link-remove").is_some());
    assert_eq!(
        s.with(|e, cx| e
            .pptx()
            .unwrap()
            .link_panel
            .as_ref()
            .unwrap()
            .url
            .read(cx)
            .value()
            .to_string()),
        "https://example.com/two"
    );
    buttons::click(&mut s, "link-remove");
    assert!(s.with(|e, _| {
        let sh = e.slide().shape(shape).unwrap();
        let body = if let ibis_doc_model::ShapeKind::Table(t) = &sh.kind {
            &t.rows[0].cells[0].text
        } else {
            sh.text_body().unwrap()
        };
        body.paragraphs[0]
            .runs
            .iter()
            .any(|r| r.style.link == Some(SlideLink::Url("https://example.com/one".into())))
            && !body.paragraphs[0]
                .runs
                .iter()
                .any(|r| r.style.link == Some(SlideLink::Url("https://example.com/two".into())))
    }));
}

fn docx_short() {
    let _probe = buttons::Probe::new();
    let _urls = UrlCapture::new();
    let mut s = Scenario::start_view();
    s.menu("file.new.document");
    let para = s
        .document()
        .body
        .iter()
        .find_map(|b| {
            if let Block::Paragraph(p) = b {
                Some(p.id)
            } else {
                None
            }
        })
        .unwrap();
    s.click_text(FlowPos::new(para, 0));
    s.type_text("before Go after");
    s.update(|e, w, cx| {
        e.set_caret(FlowPos::new(para, 7), false, cx);
        e.set_caret(FlowPos::new(para, 9), true, cx);
        e.open_link(w, cx);
        let input = e
            .doc_state()
            .unwrap()
            .link
            .as_ref()
            .unwrap()
            .address
            .clone();
        input.update(cx, |i, cx| i.set_value("https://example.com/short", w, cx));
    });
    buttons::click(&mut s, "link-apply");
    for zoom in [0.8, 1., 1.5] {
        s.update(|e, w, cx| {
            e.set_zoom(Some(zoom), w, cx);
            e.set_caret(FlowPos::new(para, 7), false, cx);
            e.scroll_to_caret(cx);
        });
        for offset in 7..9 {
            for fraction in [0.2, 0.8] {
                let p = s.with(|e, _| {
                    let lines = e.flow_lines();
                    let (page, x, y, h) = e.caret_box(&lines, FlowPos::new(para, offset)).unwrap();
                    let (_, end, _, _) =
                        e.caret_box(&lines, FlowPos::new(para, offset + 1)).unwrap();
                    let (o, k) = e.page_origin(page).unwrap();
                    (
                        f32::from(o.x) + (x + (end - x) * fraction) * k,
                        f32::from(o.y) + (y + h / 2.) * k,
                    )
                });
                s.hover(p);
                assert!(
                    s.with(|e, _| e.link_hover),
                    "short link, zoom={zoom}, offset={offset}, fraction={fraction}"
                );
                s.right_click(p);
                if !s.with(|e, _| e.link_popup.is_some()) {
                    s.screenshot("missing-short-link-menu");
                }
                assert!(
                    s.with(|e, _| e.link_popup.is_some()),
                    "menu zoom={zoom} offset={offset} fraction={fraction} point={p:?}"
                );
                s.key("escape");
                s.click(p);
                assert_eq!(UrlCapture::last(), "https://example.com/short");
            }
        }
    }
    s.screenshot("short-link-in-body");
}
fn pptx_auto() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.double_click_shape(0);
    s.key("cmd-a");
    s.type_text("https://example.com ");
    assert!(s.with(|e, _| {
        e.slide().shapes[0]
            .text_body()
            .unwrap()
            .paragraphs
            .iter()
            .flat_map(|p| p.runs.iter())
            .any(|r| {
                r.style.link.is_some()
                    && r.style.underline
                    && r.style.color == Some(ibis_doc_model::Rgb(0x0563c1))
            })
    }));
    s.screenshot("autolink-blue-underline");
}
