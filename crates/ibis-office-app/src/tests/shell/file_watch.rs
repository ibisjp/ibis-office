// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A file changed by another app while it is open (the question of shell/file_watch.rs), and a file that
//! is already open being opened again (shell/open_guard.rs: its window is raised, none is made).
//! The other app is another process (`cp`, `mv`) writing the file the window has open.
use std::path::{Path, PathBuf};

use super::{Scenario, button_probe as buttons};
use crate::docx::scenarios::DocSteps;
use crate::shell::dialog::InteractionLayer;
use crate::xlsx::scenarios::SheetSteps;
use gpui_kit::*;
use ibis_pptx::Kind;
use crate::docx::ext::*;
use crate::xlsx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::external_update_unchanged_asks_yes_no", unchanged_asks_yes_no),
        ("shell::external_update_changed_asks_three_ways", changed_asks_three_ways),
        ("shell::external_update_enter_and_escape_answer", enter_and_escape_answer),
        ("shell::external_update_no_is_not_asked_again", no_is_not_asked_again),
        ("shell::external_update_own_save_is_ignored", own_save_is_ignored),
        ("shell::external_update_replaced_by_rename", replaced_by_rename),
        ("shell::external_update_os_watcher_reaches_the_window", os_watcher_reaches_the_window),
        ("shell::external_update_keeps_the_view_document", view_kept_document),
        ("shell::external_update_keeps_the_view_long_document", view_kept_long_document),
        ("shell::external_update_keeps_the_view_workbook", view_kept_workbook),
        ("shell::external_update_keeps_the_view_presentation", view_kept_presentation),
        ("shell::open_guard_same_file", same_file),
        ("shell::open_guard_raises_the_open_window", raises_the_open_window),
    ]
}

/// A sample of each kind, and another sample of the kind to stand for what the other app wrote.
fn samples(kind: Kind) -> (&'static str, &'static str) {
    match kind {
        Kind::Document => ("test/editing/docx-report.docx", "test/editing/docx-index-basic.docx"),
        Kind::Workbook => ("test/editing/xlsx-sales.xlsx", "test/editing/xlsx-cell-images-excel.xlsx"),
        Kind::Presentation => ("test/editing/pptx-comments-modern.pptx", "test/editing/pptx-footer-by-powerpoint.pptx"),
    }
}

const KINDS: [Kind; 3] = [Kind::Document, Kind::Workbook, Kind::Presentation];

fn root(path: &str) -> PathBuf {
    crate::shell::scenario::repo_root().join(path)
}

/// What a window shows, to tell one file from another.
fn fingerprint(s: &mut Scenario) -> String {
    s.with(|e, _| shape_fingerprint(e.presentation()))
}

fn shape_fingerprint(p: &ibis_doc_model::Presentation) -> String {
    // A document's pages are made while it is shown: its blocks tell it apart.
    let blocks = p.flow.as_ref().map(|f| f.body.len());
    format!("{blocks:?} {:?}", p.slides.iter().filter(|_| blocks.is_none()).map(|slide| slide.shapes.len()).collect::<Vec<_>>())
}

/// What the file's content shows when it is read.
fn file_fingerprint(path: &Path) -> String {
    shape_fingerprint(&crate::read_file(path).expect("a readable file").presentation)
}

fn file_of(s: &mut Scenario) -> PathBuf {
    s.with(|e, _| e.path.clone()).expect("the file has a path")
}

/// Another process writes `from` over `to`.
fn other_app_copies(from: &Path, to: &Path) {
    assert!(std::process::Command::new("cp").arg(from).arg(to).status().expect("cp").success());
}

/// Another process writes `from` to a temporary file and renames it over `to` (the way most apps save).
fn other_app_replaces(from: &Path, to: &Path) {
    let temporary = to.with_file_name(".other-app-saving.tmp");
    assert!(std::process::Command::new("cp").arg(from).arg(&temporary).status().expect("cp").success());
    assert!(std::process::Command::new("mv").arg(&temporary).arg(to).status().expect("mv").success());
}

fn check(s: &mut Scenario) {
    s.update(|e, _, cx| e.check_external_update(cx));
}

fn dialog_open(s: &mut Scenario) -> bool {
    s.with(|e, _| e.watch.dialog && e.interaction_layer() == InteractionLayer::FileChanged)
}

fn edit(s: &mut Scenario, kind: Kind) {
    match kind {
        Kind::Document => {
            let (id, _) = s.paragraph(0);
            s.click_text(ibis_doc_model::FlowPos::new(id, 0));
            s.type_text("unsaved");
        }
        Kind::Workbook => {
            s.click_cell("A1");
            s.type_text("unsaved");
            s.key("enter");
        }
        Kind::Presentation => s.update(|e, _, cx| {
            let id = e.document.ids().new_shape_id();
            let frame = ibis_doc_model::EmuRect::new(0, 0, 914_400, 914_400);
            e.execute(vec![ibis_doc_model::Command::InsertShape { slide: e.current_slide, index: 0, shape: ibis_doc_model::Shape::text_box(id, frame, "unsaved") }], cx);
        }),
    }
    assert!(s.with(|e, _| e.is_dirty()));
}

/// How many pixels of the button `id` are close to `rgb` (a button's fill, border or text).
fn pixels_like(s: &mut Scenario, shot: &Path, id: &str, rgb: [u8; 3]) -> usize {
    pixels_where(s, shot, id, |p| (0..3).all(|i| (i32::from(p[i]) - i32::from(rgb[i])).abs() < 40))
}

fn pixels_where(s: &mut Scenario, shot: &Path, id: &str, matches: impl Fn(&image::Rgba<u8>) -> bool) -> usize {
    let b = buttons::bounds(id).unwrap_or_else(|| panic!("no button {id}"));
    let image = image::open(shot).unwrap().to_rgba8();
    let viewport = s.update(|_, w, _| f32::from(w.viewport_size().width));
    let scale = image.width() as f32 / viewport;
    let mut n = 0;
    for y in (f32::from(b.origin.y) * scale) as u32..(f32::from(b.bottom()) * scale) as u32 {
        for x in (f32::from(b.origin.x) * scale) as u32..(f32::from(b.right()) * scale) as u32 {
            let p = image.get_pixel(x, y);
            if matches(p) {
                n += 1;
            }
        }
    }
    n
}

/// The external-update question is an important confirmation: its `backdrop` is ON, so the window behind is dimmed.
/// Pixels that were white at the left edge (away from the box) are darker with the question open.
fn assert_backdrop_dims(before: &Path, after: &Path, kind: Kind) {
    let (a, b) = (image::open(before).unwrap().to_rgba8(), image::open(after).unwrap().to_rgba8());
    assert_eq!(a.dimensions(), b.dimensions());
    let (w, h) = a.dimensions();
    let mut white = 0;
    for y in h * 3 / 10..h * 9 / 10 {
        for x in 0..(w * 3 / 100).max(4) {
            if a.get_pixel(x, y).0[..3].iter().all(|c| *c >= 240) {
                white += 1;
                let p = b.get_pixel(x, y).0;
                assert!(p[..3].iter().all(|c| *c <= 190), "{kind:?}: the backdrop dims the window behind ({p:?} at {x},{y})");
            }
        }
    }
    assert!(white > 20, "{kind:?}: {white} white pixels to compare");
}

/// Nothing unsaved here: the question has はい (the primary button) and いいえ, and はい reads the file again.
fn unchanged_asks_yes_no() {
    let _probe = buttons::Probe::new();
    for kind in KINDS {
        let (a, b) = samples(kind);
        let mut s = Scenario::open(a);
        s.size(1200., 720.);
        let path = file_of(&mut s);
        let before = fingerprint(&mut s);
        assert!(!s.with(|e, _| e.is_dirty()));
        // Nothing changed on disk: nothing is asked.
        check(&mut s);
        assert!(!dialog_open(&mut s), "{kind:?}: no change, no question");
        let plain = s.screenshot(&format!("unchanged-before-{kind:?}"));
        other_app_copies(&root(b), &path);
        check(&mut s);
        assert!(dialog_open(&mut s), "{kind:?}: the file was changed by another app");
        let ids = buttons::all();
        assert!(ids.contains(&"file-changed-yes".to_string()) && ids.contains(&"file-changed-no".to_string()), "{kind:?}: {ids:?}");
        assert!(!ids.contains(&"file-changed-discard".to_string()) && !ids.contains(&"file-changed-save-as".to_string()), "{kind:?}: {ids:?}");
        let shot = s.screenshot(&format!("unchanged-{kind:?}"));
        assert_backdrop_dims(&plain, &shot, kind);
        // はい is the primary button: filled with the primary colour (not white, not red).
        let yes = pixels_where(&mut s, &shot, "file-changed-yes", |p| i32::from(p[2]) > i32::from(p[0]) + 50 && p[2] > 120);
        let yes_other = pixels_like(&mut s, &shot, "file-changed-yes", [0xd9, 0x30, 0x25]);
        assert!(yes > 200 && yes_other == 0, "{kind:?}: yes is primary ({yes} primary pixels, {yes_other} red)");
        assert_eq!(s.with(|e, _| e.file_name.clone().map(|n| n.to_string())), Some(path.file_name().unwrap().to_string_lossy().into_owned()));
        buttons::click(&mut s, "file-changed-yes");
        assert!(!dialog_open(&mut s), "{kind:?}: answered");
        s.settle();
        let after = fingerprint(&mut s);
        // (A document's body is shown with its pages made: it is told from the old one, not counted as read.)
        if kind != Kind::Document {
            assert_eq!(after, file_fingerprint(&path), "{kind:?}: the window shows what the other app wrote");
        }
        assert_ne!(after, before, "{kind:?}: the samples differ");
        assert_eq!(s.with(|e, _| e.path.clone()), Some(path.clone()), "{kind:?}: still that file");
        assert!(!s.with(|e, _| e.is_dirty()));
        // Read again: what it is now is known, nothing more is asked.
        check(&mut s);
        assert!(!dialog_open(&mut s), "{kind:?}: the new content is the window's");
    }
}

/// The keyboard answers the question as the buttons do: Esc is いいえ (the window keeps what it shows), Enter is はい (the file is
/// read again). With unsaved edits there is no primary button, and Enter does nothing.
fn enter_and_escape_answer() {
    for kind in KINDS {
        let (a, b) = samples(kind);
        let mut s = Scenario::open(a);
        s.size(1200., 720.);
        let path = file_of(&mut s);
        let before = fingerprint(&mut s);
        other_app_copies(&root(b), &path);
        check(&mut s);
        assert!(dialog_open(&mut s), "{kind:?}");
        s.key("escape");
        assert!(!dialog_open(&mut s), "{kind:?}: Esc answers no");
        assert_eq!(fingerprint(&mut s), before, "{kind:?}: no keeps the window's content");
        drop(s);
        // Enter reads it.
        let mut s = Scenario::open(a);
        s.size(1200., 720.);
        let path = file_of(&mut s);
        let before = fingerprint(&mut s);
        other_app_copies(&root(b), &path);
        check(&mut s);
        assert!(dialog_open(&mut s), "{kind:?}");
        s.key("enter");
        assert!(!dialog_open(&mut s), "{kind:?}: Enter answers yes");
        s.settle();
        assert_ne!(fingerprint(&mut s), before, "{kind:?}: yes read the file again");
        assert!(!s.with(|e, _| e.is_dirty()));
        drop(s);
        // Unsaved edits: no primary button; the keyboard starts on the first one that is not red (保存して開く), Tab moves on, Esc is no.
        let mut s = Scenario::open(a);
        s.size(1200., 720.);
        let path = file_of(&mut s);
        edit(&mut s, kind);
        other_app_copies(&root(b), &path);
        check(&mut s);
        assert!(dialog_open(&mut s), "{kind:?}");
        assert_eq!(s.with(|e, _| e.message_box_focus()), 1, "{kind:?}: the ring starts on save-as, not on the red button");
        s.key("tab");
        assert_eq!(s.with(|e, _| e.message_box_focus()), 2, "{kind:?}: Tab: to no");
        s.key("escape");
        assert!(!dialog_open(&mut s), "{kind:?}: Esc answers no");
        assert!(s.with(|e, _| e.is_dirty()), "{kind:?}: the edits are kept");
    }
}

/// Unsaved edits: 今の変更を破棄して読み込み直す (red), 名前をつけて保存して開く and いいえ; the first reads the file again.
fn changed_asks_three_ways() {
    let _probe = buttons::Probe::new();
    for kind in KINDS {
        let (a, b) = samples(kind);
        let mut s = Scenario::open(a);
        s.size(1200., 720.);
        let path = file_of(&mut s);
        edit(&mut s, kind);
        other_app_copies(&root(b), &path);
        check(&mut s);
        assert!(dialog_open(&mut s), "{kind:?}");
        let ids = buttons::all();
        for id in ["file-changed-discard", "file-changed-save-as", "file-changed-no"] {
            assert!(ids.contains(&id.to_string()), "{kind:?}: {id} in {ids:?}");
        }
        assert!(!ids.contains(&"file-changed-yes".to_string()), "{kind:?}: {ids:?}");
        let shot = s.screenshot(&format!("changed-{kind:?}"));
        // The dangerous answer is the common red button: red text and border on white.
        let red = pixels_like(&mut s, &shot, "file-changed-discard", [0xd9, 0x30, 0x25]);
        assert!(red > 100, "{kind:?}: discard is red ({red} red pixels)");
        let no_red = pixels_like(&mut s, &shot, "file-changed-no", [0xd9, 0x30, 0x25]);
        assert_eq!(no_red, 0, "{kind:?}: No is a normal button");
        // The document behind is not reachable while the question stands.
        assert!(s.with(|e, _| e.is_dirty()));
        buttons::click(&mut s, "file-changed-discard");
        s.settle();
        assert!(!dialog_open(&mut s));
        if kind != Kind::Document {
            assert_eq!(fingerprint(&mut s), file_fingerprint(&path), "{kind:?}: the other app's file, the edit dropped");
        }
        assert!(!s.with(|e, _| e.is_dirty()), "{kind:?}: nothing unsaved after reading it again");
    }
}

/// いいえ and Escape do nothing, and the same change is not asked again; a later change is.
fn no_is_not_asked_again() {
    let _probe = buttons::Probe::new();
    let (a, b) = samples(Kind::Workbook);
    let mut s = Scenario::open(a);
    s.size(1200., 720.);
    let path = file_of(&mut s);
    edit(&mut s, Kind::Workbook);
    let mine = fingerprint(&mut s);
    other_app_copies(&root(b), &path);
    check(&mut s);
    assert!(dialog_open(&mut s));
    buttons::click(&mut s, "file-changed-no");
    assert!(!dialog_open(&mut s));
    assert_eq!(fingerprint(&mut s), mine, "No keeps the window as it is");
    assert!(s.with(|e, _| e.is_dirty()), "and its edits");
    check(&mut s);
    check(&mut s);
    assert!(!dialog_open(&mut s), "the same change is not asked again");
    // Another change by the other app: asked again. Escape answers いいえ.
    other_app_copies(&root(samples(Kind::Workbook).0), &path);
    check(&mut s);
    assert!(dialog_open(&mut s), "a different update asks again");
    s.key("escape");
    assert!(!dialog_open(&mut s), "Escape is No");
    assert_eq!(fingerprint(&mut s), mine);
    check(&mut s);
    assert!(!dialog_open(&mut s));
    // While a question stands, a newer change does not make a second one; Enter on a window with edits does not answer it.
    other_app_copies(&root(b), &path);
    check(&mut s);
    assert!(dialog_open(&mut s));
    s.key("escape");
    assert!(!dialog_open(&mut s));
}

/// What this app writes itself is not a change by another app, though it renames a file over the file.
fn own_save_is_ignored() {
    for kind in KINDS {
        let (a, _) = samples(kind);
        let mut s = Scenario::open(a);
        s.size(1200., 720.);
        let path = file_of(&mut s);
        edit(&mut s, kind);
        s.key("cmd-s");
        s.settle();
        assert!(!s.with(|e, _| e.is_dirty()), "{kind:?}: saved");
        check(&mut s);
        assert!(!dialog_open(&mut s), "{kind:?}: the app's own save is not asked about");
        // Saved again, later: the same.
        edit(&mut s, kind);
        s.key("cmd-s");
        check(&mut s);
        assert!(!dialog_open(&mut s), "{kind:?}: and the next save");
        // The other app touches the file or writes the very same bytes: the time changed, not the content.
        let bytes = std::fs::read(&path).unwrap();
        let copy = path.with_file_name("same-bytes.tmp");
        std::fs::write(&copy, &bytes).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        other_app_replaces(&copy, &path);
        check(&mut s);
        assert!(!dialog_open(&mut s), "{kind:?}: the same content is not a change");
        // What it really changes is.
        other_app_copies(&root(samples(kind).1), &path);
        check(&mut s);
        assert!(dialog_open(&mut s), "{kind:?}: a real change is asked about");
    }
}

/// An app that saves through a temporary file renamed over the file is a change of the file's path too.
fn replaced_by_rename() {
    let (a, b) = samples(Kind::Document);
    let mut s = Scenario::open(a);
    let path = file_of(&mut s);
    other_app_replaces(&root(b), &path);
    check(&mut s);
    assert!(dialog_open(&mut s));
    s.update(|e, _, cx| e.dismiss_external_update(cx));
    assert!(!dialog_open(&mut s));
    // A deleted file is not handled: nothing is asked.
    std::fs::remove_file(&path).unwrap();
    check(&mut s);
    assert!(!dialog_open(&mut s), "a deleted file is left alone");
}

/// The operating system's watcher tells the window (no direct call of the check): the events of the folder are
/// taken after the debounce, and the question appears in the window whose file it is, not in another.
fn os_watcher_reaches_the_window() {
    let (a, b) = samples(Kind::Document);
    let mut s = Scenario::open(a);
    let (xa, _) = samples(Kind::Workbook);
    // Another window with another file, in the same folder.
    let other_path = file_of(&mut s).with_file_name("other-window.xlsx");
    std::fs::copy(root(xa), &other_path).unwrap();
    s.with_app(|cx| crate::open_path_in_app(other_path.clone(), cx));
    s.wait(300);
    s.with_app(|cx| crate::shell::file_watch::start(cx));
    let path = file_of(&mut s);
    // The window watches the folder from the moment it has the path.
    std::thread::sleep(std::time::Duration::from_millis(300));
    other_app_replaces(&root(b), &path);
    let mut asked = false;
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        s.wait(500);
        if dialog_open(&mut s) {
            asked = true;
            break;
        }
    }
    assert!(asked, "the watcher's event reached the window");
    // The window of the other file was not asked.
    let others = s.with_app(|cx| {
        let mut asked = 0;
        for w in cx.windows() {
            let Some(root) = w.downcast::<gpui_kit::component::Root>() else { continue };
            root.update(cx, |root, _, cx| {
                if let Ok(editor) = root.view().clone().downcast::<crate::shell::editor::SlideEditor>() {
                    asked += usize::from(editor.read(cx).watch.dialog);
                }
            })
            .ok();
        }
        asked
    });
    assert_eq!(others, 1, "only the window of the changed file asks");
    s.screenshot("os-watcher");
}

/// Paths of one file spelt in different ways are the same file; another file is not.
fn same_file() {
    use crate::shell::open_guard::same_file;
    let dir = crate::shell::scenario::repo_root().join("target/scenario/open_guard_same_file");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    let file = dir.join("Report.docx");
    std::fs::copy(root("test/editing/docx-report.docx"), &file).unwrap();
    let other = dir.join("other.docx");
    std::fs::copy(root("test/editing/docx-report.docx"), &other).unwrap();
    assert!(same_file(&file, &file));
    assert!(same_file(&file, &dir.join("sub").join("..").join("Report.docx")), "..");
    assert!(same_file(&file, &dir.join(".").join("Report.docx")), ".");
    assert!(same_file(&file, Path::new(&format!("{}/", dir.display())).join("Report.docx").as_path()), "doubled separator");
    #[cfg(unix)]
    {
        let link = dir.join("link.docx");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert!(same_file(&file, &link), "symbolic link");
        let folder_link = crate::shell::scenario::repo_root().join("target/scenario/open_guard_same_file_folder_link");
        let _ = std::fs::remove_file(&folder_link);
        std::os::unix::fs::symlink(&dir, &folder_link).unwrap();
        assert!(same_file(&file, &folder_link.join("Report.docx")), "a link to the folder");
        std::fs::hard_link(&file, dir.join("hard.docx")).unwrap();
        assert!(same_file(&file, &dir.join("hard.docx")), "hard link");
    }
    #[cfg(target_os = "macos")]
    {
        // The volume ignores case (the usual setting on a Mac).
        let upper = dir.join("REPORT.DOCX");
        if upper.exists() {
            assert!(same_file(&file, &upper), "upper case on a volume that ignores case");
        }
    }
    assert!(!same_file(&file, &other), "a copy is another file");
    assert!(!same_file(&file, &dir.join("missing.docx")), "a file that is not there");
}

fn windows(s: &mut Scenario) -> usize {
    s.window_count()
}

/// Opening a file that a window has open (Finder / `open`, the Open dialog, the recent files, a drop) makes no window: it raises that window.
fn raises_the_open_window() {
    let (a, b) = samples(Kind::Document);
    let mut s = Scenario::open(a);
    let path = file_of(&mut s);
    let n = windows(&mut s);
    let first = s.with_app(|cx| cx.windows()[0]);
    // Finder, `open`: the same path.
    s.with_app(|cx| crate::open_path_in_app(path.clone(), cx));
    s.settle();
    assert_eq!(windows(&mut s), n, "no second window on the same file");
    // Spelt another way (through `..`, a symbolic link).
    let dir = path.parent().unwrap().to_path_buf();
    let dotted = dir.join("..").join(dir.file_name().unwrap()).join(path.file_name().unwrap());
    s.with_app(|cx| crate::open_path_in_app(dotted, cx));
    s.settle();
    assert_eq!(windows(&mut s), n, "..");
    #[cfg(unix)]
    {
        let link = dir.join("link-to-the-file.docx");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&path, &link).unwrap();
        s.with_app(|cx| crate::open_path_in_app(link, cx));
        s.settle();
        assert_eq!(windows(&mut s), n, "symbolic link");
    }
    // The Open dialog / recent files / the start view: the same entry of the window.
    s.update(|e, w, cx| e.open_path_or_raise(path.clone(), w, cx));
    assert_eq!(windows(&mut s), n, "the window's own file is not opened again over its edits");
    // A drop on the window.
    s.drop_files((300., 300.), vec![path.clone()]);
    s.settle();
    assert_eq!(windows(&mut s), n, "a drop of its own file");
    // Another file does open (in a new window); then the first file brings its window to the front.
    let other = dir.join("second.docx");
    std::fs::copy(root(b), &other).unwrap();
    s.with_app(|cx| crate::open_path_in_app(other.clone(), cx));
    s.wait(300);
    assert_eq!(windows(&mut s), n + 1, "another file opens a window");
    // The file still loading is taken too: opening it again at once makes no third window.
    s.with_app(|cx| crate::open_path_in_app(other.clone(), cx));
    s.settle();
    assert_eq!(windows(&mut s), n + 1, "a file being loaded");
    let second = s.with_app(|cx| cx.active_window()).expect("the new window is in front");
    s.with_app(|cx| crate::open_path_in_app(path.clone(), cx));
    s.settle();
    assert_eq!(windows(&mut s), n + 1);
    let front = s.with_app(|cx| cx.active_window());
    assert_eq!(front, Some(first), "the first file's window is in front now");
    assert_ne!(front, Some(second));
    // From the recent files in the other window: also raises.
    s.with_app(|cx| crate::open_path_in_app(other.clone(), cx));
    s.settle();
    assert_eq!(s.with_app(|cx| cx.active_window()), Some(second), "and back");
    assert_eq!(windows(&mut s), n + 1);
}

// ----- 読み込み直したあとも同じ画面（shell/view_state.rs） -------------------------------------------------

/// What the window shows of its view: scroll, zoom, page (sheet, slide) and the selection.
#[derive(Debug, PartialEq)]
struct View {
    scroll: (i32, i32),
    zoom: Option<i32>,
    page: usize,
    selection: String,
}

fn view_of(s: &mut Scenario) -> View {
    let selection = if s.with(|e, _| e.is_workbook()) {
        let range = s.with(|e, _| e.selected_range());
        format!("{:?} {range:?}", s.selected())
    } else if s.with(|e, _| e.is_document()) {
        format!("{:?}", s.selection())
    } else {
        s.with(|e, _| format!("{:?}", e.selected_ids()))
    };
    s.with(|e, _| {
        let offset = e.workspace_scroll.offset();
        View {
            scroll: (f32::from(offset.x).round() as i32, f32::from(offset.y).round() as i32),
            zoom: e.zoom.map(|z| (z * 100.0).round() as i32),
            page: e.presentation().slide_index(e.current_slide).unwrap_or(0) + 1,
            selection,
        }
    })
}

/// Zoom to `zoom` and scroll to `(x, y)` pixels (as far as the content goes).
fn view_scroll(s: &mut Scenario, zoom: f32, x: f32, y: f32) {
    s.update(|e, window, cx| {
        e.set_zoom(Some(zoom), window, cx);
        e.workspace_scroll.set_offset(point(px(-x), px(-y)));
        e.clamp_workspace_scroll();
    });
    s.settle();
}

/// Another app writes `from` over the open file; the window asks and the answer is はい.
fn answer_yes_to_the_update(s: &mut Scenario, from: &str) {
    let path = file_of(s);
    other_app_copies(&root(from), &path);
    check(s);
    assert!(dialog_open(s), "the file was changed by another app");
    // (Zooming a worksheet is a change of the file, so the question may be the one with unsaved edits.)
    let yes = if buttons::all().contains(&"file-changed-yes".to_string()) { "file-changed-yes" } else { "file-changed-discard" };
    buttons::click(s, yes);
    s.settle();
    s.settle();
    assert!(!dialog_open(s));
}

/// A document: the scroll (x and y), the zoom and the caret stay when the same file is written again; a caret
/// that is not in the new file is dropped, the scroll is kept as far as the new content goes.
fn view_kept_document() {
    let _probe = buttons::Probe::new();
    let sample = "test/editing/docx-chapter-pages.docx";
    let mut s = Scenario::open(sample);
    s.size(1200., 720.);
    let (id, text) = s.paragraph(3);
    assert!(text.chars().count() > 2, "a paragraph to put the caret in");
    s.click_text(ibis_doc_model::FlowPos::new(id, 2));
    view_scroll(&mut s, 1.5, 40., 900.);
    let before = view_of(&mut s);
    assert_ne!(before.scroll, (0, 0), "scrolled away from the start: {before:?}");
    assert_eq!(before.zoom, Some(150));
    assert!(before.selection.contains("Some"), "{before:?}");
    answer_yes_to_the_update(&mut s, sample);
    assert_eq!(view_of(&mut s), before, "the same file read again: the same view");
    // The caret of another document: a paragraph that is not there (or an offset beyond it) is not put back.
    answer_yes_to_the_update(&mut s, "test/editing/docx-paste-source.docx");
    let after = view_of(&mut s);
    assert_eq!(after.zoom, Some(150), "{after:?}");
    assert!(after.scroll.0 <= 0 && after.scroll.1 <= 0, "{after:?}");
    if let Some((caret, _)) = s.selection() {
        let ok = s.with(|e, _| {
            e.flow().and_then(|d| ibis_doc_model::flow_command::paragraph(d, caret.para)).is_some_and(|p| caret.offset <= p.len())
        });
        assert!(ok, "a caret that is put back is inside the text: {caret:?}");
    }
}

/// A long document (many pages), scrolled far down at the fit-to-window zoom and again at a chosen zoom, no caret: the
/// scroll stays when the same file is written again.
fn view_kept_long_document() {
    let _probe = buttons::Probe::new();
    let sample = "test/benchmark/securities-registration-statement.docx";
    let mut s = Scenario::open(sample);
    s.size(1200., 720.);
    for zoom in [1.0, 0.6] {
        view_scroll(&mut s, zoom, 0., 6000.);
        let before = view_of(&mut s);
        assert!(before.scroll.1 < -3000, "scrolled far down: {before:?}");
        answer_yes_to_the_update(&mut s, sample);
        assert_eq!(view_of(&mut s), before, "the same file read again: the same view at zoom {zoom}");
    }
}

/// A workbook: the sheet, its scroll, the zoom and every sheet's selection stay; a sheet that is gone gives way to the
/// one at the same position.
fn view_kept_workbook() {
    let _probe = buttons::Probe::new();
    let sample = "test/editing/xlsx-sales.xlsx";
    let mut s = Scenario::open(sample);
    s.size(1200., 720.);
    // 月次売上, 商品, メモ: a cell chosen on the first, then the second shown with its own cell, zoom and scroll.
    s.click_cell("B3");
    s.page(2);
    s.click_cell("C5");
    view_scroll(&mut s, 1.5, 30., 160.);
    let before = view_of(&mut s);
    assert_eq!(before.page, 2);
    assert_eq!(before.zoom, Some(150));
    assert_ne!(before.scroll, (0, 0), "{before:?}");
    answer_yes_to_the_update(&mut s, sample);
    assert_eq!(view_of(&mut s), before, "the same file read again: the same view");
    s.page(1);
    assert_eq!(s.selected(), "B3", "the first sheet kept its own selection");
    s.page(2);
    // The sheet 商品 is the third of the other file: found by its name.
    view_scroll(&mut s, 1.5, 30., 160.);
    answer_yes_to_the_update(&mut s, "test/editing/xlsx-nondynamic-metadata.xlsx");
    let moved = view_of(&mut s);
    assert_eq!(moved.page, 3, "found by its name: {moved:?}");
    assert_eq!(moved.zoom, Some(150));
    assert_eq!(moved.scroll, before.scroll);
    assert_eq!(s.selected(), "C5");
    // Another file with none of the names (two sheets): the sheet at the same place, or the last.
    answer_yes_to_the_update(&mut s, sample);
    s.page(3);
    s.click_cell("D7");
    view_scroll(&mut s, 0.8, 0., 100.);
    answer_yes_to_the_update(&mut s, "test/editing/xlsx-formats.xlsx");
    let gone = view_of(&mut s);
    assert_eq!(gone.page, 2, "the third sheet is gone: the last one is shown ({gone:?})");
    assert_eq!(gone.zoom, Some(80));
    assert_eq!(s.selected(), "D7");
    assert!(!s.with(|e, _| e.is_dirty()), "putting the view back is not an edit");
}

/// A presentation: the slide, the selected shape, the zoom and the scroll stay; fewer slides: the nearest one.
fn view_kept_presentation() {
    let _probe = buttons::Probe::new();
    let sample = "test/editing/pptx-plan.pptx";
    let mut s = Scenario::open(sample);
    s.size(1200., 720.);
    s.page(3);
    s.select_shape(0);
    view_scroll(&mut s, 2.0, 60., 80.);
    let before = view_of(&mut s);
    assert_eq!(before.page, 3);
    assert_ne!(before.selection, "[]", "a shape is selected: {before:?}");
    assert_ne!(before.scroll, (0, 0), "{before:?}");
    answer_yes_to_the_update(&mut s, sample);
    assert_eq!(view_of(&mut s), before, "the same file read again: the same view");
    // Two slides in the new file (pptx-comments-modern): slide 4 is gone, the last one is shown.
    s.page(4);
    view_scroll(&mut s, 2.0, 60., 80.);
    answer_yes_to_the_update(&mut s, "test/editing/pptx-comments-modern.pptx");
    let fewer = view_of(&mut s);
    assert_eq!(fewer.page, 2, "the nearest slide: {fewer:?}");
    assert_eq!(fewer.zoom, Some(200));
    assert!(!s.with(|e, _| e.is_dirty()));
}
