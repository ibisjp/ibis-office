// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenario tests: a user's series of steps done through the window —
//! clicks, keys, the menus, the toolbar, the sidebar — with the window's
//! state, the model and the saved file checked along the way
//! (`tasks/doc/editing-plan.md` §2.4.1).
//!
//! The window runs headless (GPUI's test support, as `ui_snapshot.rs`), at
//! the size you set, and is drawn after each step, so what a step sees is
//! what a user would. Write a scenario as a `#[test]` in your format's
//! directory (`src/<format>/scenarios.rs`, see `tasks/doc/editing-status.md`)
//! as a plain function, and list it in `FormatEditor::scenarios` (they run
//! from `tests/scenarios.rs` on the main thread, which GPUI's Mac platform
//! needs; `cargo test --release` runs them):
//!
//! ```ignore
//! use crate::shell::scenario::Scenario;
//!
//! pub(crate) fn type_into_a_new_text_box_and_make_it_bold() {
//!     let mut s = Scenario::open("test/editing/pptx-plan.pptx");
//!     s.toolbar("tool-text");                       // a toolbar button, by id
//!     s.drag((400.0, 520.0), (700.0, 580.0));       // window coordinates
//!     s.type_text("Hello");
//!     s.key("escape");
//!     s.toolbar("bold");
//!     assert!(s.toolbar_state("bold").unwrap().on);
//!     s.menu("file.save");                          // a menu item, by id
//!     let saved = s.reopen();                       // the file read again
//! }
//! ```
//!
//! Steps: [`Scenario::click`], [`double_click`](Scenario::double_click),
//! [`drag`](Scenario::drag), [`right_click`](Scenario::right_click),
//! [`shift_click`](Scenario::shift_click), [`click_shape`](Scenario::click_shape) (a
//! shape of the slide by index, at its middle), [`key`](Scenario::key)
//! (`"cmd-b"`, `"enter"`), [`type_text`](Scenario::type_text),
//! [`ime`](Scenario::ime) (composing, then committing, through the format's
//! input hooks), [`toolbar`](Scenario::toolbar), [`menu`](Scenario::menu),
//! [`page`](Scenario::page), [`save_as`](Scenario::save_as),
//! [`screenshot`](Scenario::screenshot). Checks: [`with`](Scenario::with)
//! (read the editor and its model), [`toolbar_state`](Scenario::toolbar_state),
//! [`menu_state`](Scenario::menu_state), [`status_texts`](Scenario::status_texts),
//! [`reopen`](Scenario::reopen). Anything else: [`update`](Scenario::update)
//! (the editor, its window and context).
//!
//! Scenarios run one at a time, on the main thread. [`Scenario::open`]
//! opens a copy of the test file (in `target/scenario/<test name>/`), so
//! saving never changes the test files; a scenario that changes a file
//! under `test/` anyway fails.

#![allow(dead_code)] // The driver's API, for the formats' scenarios.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use gpui_kit::component::Root;
use gpui_kit::*;

use crate::shell::chrome::{MenuEntry, ToolItem};
use crate::shell::editor::SlideEditor;
use crate::shell::format::with_format;

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// Every scenario: the shell's examples and each format's
/// (`FormatEditor::scenarios`), as (name, function).
#[path = "../tests/shell/dialog_apply.rs"]
pub(crate) mod dialog_apply;
#[path = "../tests/shell/button_probe.rs"]
pub(crate) mod button_probe;
#[path = "../tests/shell/hyperlinks.rs"]
mod hyperlink_scenarios;
#[path = "../tests/shell/panel_dialog.rs"]
mod panel_dialog_scenarios;
#[path = "../tests/shell/merge_workflow.rs"]
mod merge_workflow_scenarios;
#[path = "../tests/shell/ui_refinements.rs"]
mod ui_refinements;
#[path = "../tests/shell/layout_refinements.rs"]
mod layout_refinements;
#[path = "../tests/shell/xlsx_ink_followup.rs"]
mod xlsx_ink_followup;
#[path="../tests/shell/ui_followup.rs"]
mod ui_followup;
#[path="../tests/shell/issues_33_plus.rs"]
mod issues_33_plus;
#[path="../tests/shell/issues59_62.rs"]
mod issues59_62;
#[path="../tests/shell/ui_20261028.rs"]
mod ui_20261028;
#[path="../tests/shell/ui_20261029_align.rs"]
mod ui_20261029_align;
#[path="../tests/shell/editing_20261028.rs"]
mod editing_20261028;
#[path="../tests/shell/followup_20261028.rs"]
mod followup_20261028;
#[path="../tests/shell/issues64_65.rs"]
mod issues64_65;
#[path = "../tests/shell/dropdown.rs"]
mod dropdown;
#[path = "../tests/shell/segment_control_ui.rs"]
mod segment_control_ui;
#[path = "../tests/shell/panel_catalog.rs"]
mod panel_catalog;

#[path = "../tests/shell/menu_viewport.rs"]
mod menu_viewport;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    use crate::shell::format::FormatEditor;
    let mut all = crate::shell::scenario_examples::all();
    all.extend(crate::native_quit_scenarios::all());
    all.extend(panel_dialog_scenarios::all());
    all.push(("shell::submenu_viewport_fit",menu_viewport::run));
    all.extend(hyperlink_scenarios::all());
    all.extend(dialog_apply::all());
    all.extend(ui_refinements::all());
    all.extend(layout_refinements::all());
    all.extend(ui_followup::all());
    all.extend(issues_33_plus::all());
    all.extend(issues59_62::all());
    all.extend(ui_20261028::all());
    all.extend(ui_20261029_align::all());
    all.extend(editing_20261028::all());
    all.extend(followup_20261028::all());
    all.extend(issues64_65::all());
    all.push(("shell::dropdown_item_attributes",dropdown::run));
    all.push(("shell::toolbar_style_common_dropdown",dropdown::toolbar_style));
    all.extend(xlsx_ink_followup::all());
    all.extend(segment_control_ui::all());
    all.extend(merge_workflow_scenarios::all());
    all.extend(crate::shell::scenario_i18n::all());
    all.extend(crate::shell::scenario_fix::all());
    all.extend(crate::shell::scenario_menus::all());
    all.extend(<crate::pptx::Presentation as FormatEditor>::scenarios());
    all.extend(<crate::xlsx::Workbook as FormatEditor>::scenarios());
    all.extend(<crate::docx::Document as FormatEditor>::scenarios());
    all
}

/// Unfixed bug scenarios (corpus_report), skipped in normal `cargo test` and
/// run with `--ignored` or `--include-ignored`.
pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    let mut out: Vec<(&'static str, fn())> = vec![("shell::dump_all_right_panels", panel_catalog::dump_all_right_panels)];
    out.extend(ui_followup::ignored());
    #[cfg(target_os = "macos")]
    {
        out.extend(crate::pptx::corpus_report::ignored());
        out.extend(crate::xlsx::corpus_report::ignored());
        out.extend(crate::docx::corpus_report::ignored());
    }
    out
}

/// What the system's frameworks autoreleased during a scenario (the URLs and strings of font and file lookups: 350 000
/// of them after 40 scenarios). The scenarios run on a thread with no run loop, which would never drain them.
struct AutoreleasePool(#[cfg(target_os = "macos")] *mut objc::runtime::Object);

impl AutoreleasePool {
    fn new() -> Self {
        #[cfg(target_os = "macos")]
        {
            use objc::{class, msg_send, sel, sel_impl};
            // SAFETY: NSAutoreleasePool new/drain on this thread, released before the scenario's thread data is.
            return Self(unsafe { msg_send![class!(NSAutoreleasePool), new] });
        }
        #[cfg(not(target_os = "macos"))]
        Self()
    }
}

impl Drop for AutoreleasePool {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            use objc::{msg_send, sel, sel_impl};
            // SAFETY: the pool made by `new`.
            unsafe {
                let _: () = msg_send![self.0, drain];
            }
        }
    }
}

/// Write the ids of a format's menu items to `tasks/doc/<file>` (read by `scripts/gen-<format>-ribbon-map.py`), only when the
/// environment variable `IBIS_DUMP_MENU_IDS` is set: the scenarios do not rewrite a file of the repository on every run.
pub(crate) fn dump_menu_ids(file: &str, ids: &[String]) {
    if std::env::var_os("IBIS_DUMP_MENU_IDS").is_some() {
        std::fs::write(repo_root().join("tasks/doc").join(file), ids.join("\n")).unwrap();
    }
}

/// Run the scenarios whose names contain one of `filter` (all without
/// one), except those whose names contain a `filter` starting with `!`
/// (`!pptx::gradient`), printing each result as libtest does. The exit
/// status: 0 when all pass.
pub fn run(args: &[String]) -> i32 {
    crate::native_shape_clipboard::simulate(None);
    if args.iter().any(|arg|matches!(arg.as_str(),"--help"|"-h")) {
        println!("Usage: scenarios [--list] [--ignored | --include-ignored] [name ...] [!excluded-name ...]");
        return 0;
    }
    let only_ignored = args.iter().any(|a| a == "--ignored");
    let include_ignored = args.iter().any(|a| a == "--include-ignored");
    let filter: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    let (skip, take): (Vec<&String>, Vec<&String>) = filter.into_iter().partition(|f| f.starts_with('!'));
    let matches = |name: &str| {
        (take.is_empty() || take.iter().any(|f| name.contains(f.as_str())))
            && !skip.iter().any(|f| name.contains(&f[1..]))
    };
    let source: Vec<_> = if only_ignored {
        ignored()
    } else if include_ignored {
        all().into_iter().chain(ignored()).collect()
    } else {
        all()
    };
    let chosen: Vec<_> = source.into_iter().filter(|(name, _)| matches(name)).collect();
    if args.iter().any(|a| a == "--list") {
        // Names only, one per line, so a caller can run them in separate processes.
        for (name, _) in &chosen { println!("{name}"); }
        return 0;
    }
    let skipped_ignored: Vec<_> = if !only_ignored && !include_ignored {
        ignored().into_iter().filter(|(name, _)| matches(name)).collect()
    } else {
        Vec::new()
    };
    println!("\nrunning {} scenarios", chosen.len() + skipped_ignored.len());
    for (name, _) in &skipped_ignored {
        println!("scenario {name} ... ignored");
    }
    let mut failed = Vec::new();
    // The test files must come out as they went in (a scenario works on
    // copies of them, `Scenario::open`). One scenario's after is the next
    // one's before (listing them takes 0.1 s, twice per scenario was a
    // minute of the 333).
    let mut before = test_files();
    for (name, f) in &chosen {
        crate::native_shape_clipboard::simulate(None);
        CURRENT.with(|c| *c.borrow_mut() = name.to_string());
        // What one scenario sets (typing corrections, the user's name, the backup copy) is not the next one's start.
        crate::shell::autocorrect::reset();
        crate::shell::user_name::reset();
        crate::docx::backup::reset();
        let started = crate::clock::Instant::now();
        let pool = AutoreleasePool::new();
        crate::shell::links::CAPTURE_URLS.with(|s|*s.borrow_mut()=Some(Vec::new()));
        let result = std::panic::catch_unwind(f);
        crate::shell::links::CAPTURE_URLS.with(|s|*s.borrow_mut()=None);
        drop(pool);
        let took = started.elapsed();
        let after = test_files();
        let changed = changed_files(&before, &after);
        before = after;
        for path in &changed {
            println!("scenario {name}: changed a test file: {path}");
        }
        let ok = result.is_ok() && changed.is_empty();
        // The time it took (scripts/check-main.py lists the slowest).
        println!("scenario {name} ... {} ({:.2}s)", if ok { "ok" } else { "FAILED" }, took.as_secs_f64());
        // IBIS_SCENARIO_MEM=1: the process's memory (`footprint`'s total, as the system counts it) after each scenario, to find what a
        // scenario leaves behind.
        if std::env::var_os("IBIS_SCENARIO_MEM").is_some()
            && let Ok(out) = std::process::Command::new("footprint").args(["-p", &std::process::id().to_string()]).output()
            && let Some(line) = String::from_utf8_lossy(&out.stdout).lines().find(|l| l.contains("Footprint:"))
        {
            println!("scenario-mem {name} {}", line.split("Footprint:").nth(1).unwrap_or("").trim());
        }
        if !ok {
            failed.push(*name);
        }
    }
    // Every id the scenarios asked the UI tables for is in them (an id not
    // in them would be shown as it is), and the log of the texts asked for
    // (`IBIS_I18N_LOG=<file>`).
    // No id is drawn as it is on the screen (a string the UI laid out that is
    // an id of the tables: a label that was not translated).
    for id in drawn_text::ids_drawn() {
        println!("scenario i18n: the id {id:?} was drawn on the screen as it is");
        failed.push("i18n: an id drawn as it is");
        break;
    }
    let missing = crate::shell::i18n::log::missing_ids();
    for id in &missing {
        println!("scenario i18n: the id {id:?} is not in the tables");
    }
    if !missing.is_empty() {
        failed.push("i18n: ids missing from the tables");
    }
    if let Some(path) = std::env::var_os("IBIS_I18N_LOG") {
        let _ = std::fs::write(path, crate::shell::i18n::log::lines().join("\n") + "\n");
    }
    // IBIS_SCENARIO_HOLD=1: wait here (for `heap`/`vmmap` on the process, to see what the scenarios left in memory).
    if std::env::var_os("IBIS_SCENARIO_HOLD").is_some() {
        println!("scenario-hold pid {}", std::process::id());
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }
    println!(
        "\nscenario result: {}. {} passed; {} failed; {} ignored",
        if failed.is_empty() { "ok" } else { "FAILED" },
        chosen.len() - failed.len(),
        failed.len(),
        skipped_ignored.len()
    );
    for name in &failed {
        println!("    {name}");
    }
    if failed.is_empty() { 0 } else { 1 }
}

thread_local! {
    /// The scenario running (for the screenshots' folder).
    static CURRENT: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// The test files under the repository's `test/` (those git tracks, and new
/// files git does not ignore), each with its size and modification time
/// (nanoseconds): a scenario that writes, adds or removes one changes these.
/// Ignored files (`test/coverage/out`, `test/private`, ...) are left out, and
/// the contents are not read: reading every file of `test/` before and after
/// each scenario took gigabytes per scenario where `test/coverage/out` is
/// large. Without git, the files under `test/` except `test/coverage/out`
/// and `test/private`.
fn test_files() -> std::collections::BTreeMap<String, (u64, u128)> {
    let root = repo_root();
    let stamp = |path: &Path| {
        std::fs::metadata(path).ok().map(|m| {
            let time = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
            (m.len(), time)
        })
    };
    let mut out = std::collections::BTreeMap::new();
    let listed = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", "test"])
        .output()
        .ok()
        .filter(|o| o.status.success());
    if let Some(listed) = listed {
        for name in listed.stdout.split(|&b| b == 0).filter(|n| !n.is_empty()) {
            let name = String::from_utf8_lossy(name).into_owned();
            // A tracked file that is gone: (0, 0), different from any file.
            let s = stamp(&root.join(&name)).unwrap_or((0, 0));
            out.insert(name, s);
        }
        return out;
    }
    let skip = [root.join("test/coverage/out"), root.join("test/private")];
    let mut dirs = vec![root.join("test")];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() && !skip.contains(&path) => dirs.push(path),
                Ok(t) if t.is_file() => {
                    let name = path.strip_prefix(&root).unwrap_or(&path).display().to_string();
                    out.insert(name, stamp(&path).unwrap_or((0, 0)));
                }
                _ => {}
            }
        }
    }
    out
}

/// The files added, removed or changed from `before` to `after`.
fn changed_files(before: &std::collections::BTreeMap<String, (u64, u128)>, after: &std::collections::BTreeMap<String, (u64, u128)>) -> Vec<String> {
    let mut out: Vec<String> = before.iter().filter(|(path, hash)| after.get(*path) != Some(hash)).map(|(path, _)| path.clone()).collect();
    out.extend(after.keys().filter(|path| !before.contains_key(*path)).cloned());
    out
}

/// The repository's root (for the test files' paths).
pub(crate) fn repo_root() -> PathBuf {
    crate::shell::plain_path::canonical(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).expect("the repository")
}

/// What a toolbar button shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ToolState {
    pub on: bool,
    pub enabled: bool,
}

/// A window with a document in it, driven step by step.
pub(crate) struct Scenario {
    // Dropped in this order: the editor's handle before the application,
    // which checks that no handle is left.
    editor: Entity<SlideEditor>,
    window: WindowHandle<Root>,
    cx: HeadlessAppContext,
    name: String,
    shots: usize,
    _lock: MutexGuard<'static, ()>,
}

impl Drop for Scenario {
    fn drop(&mut self) {
        // Close the window and let the application release the editor (its document, the pictures decoded for it and the
        // render caches): otherwise each scenario's whole world stays in memory (hundreds of MB for a document with pictures).
        let editor = self.editor.clone();
        self.cx.update(|app| editor.update(app, |e, _| e.picture_watch = None));
        self.cx.update_window(self.window.into(), |_, window, _| window.remove_window()).ok();
        self.cx.run_until_parked();
    }
}

impl Scenario {
    /// A window (1200 × 720, the UI mock's size) with the file at `path`
    /// (from the repository's root) opened as 開く… opens it, with what
    /// saving needs. The file is copied to `target/scenario/<test name>/`
    /// first and the copy opened, so saving (⌘S) writes the copy, never the
    /// test file. Screenshots go to the same folder.
    pub(crate) fn open(path: &str) -> Self {
        Self::open_sized(path, 1200.0)
    }

    /// `open` in a window `width` points wide.
    pub(crate) fn open_sized(path: &str, width: f32) -> Self {
        let original = repo_root().join(path);
        let mut s = Self::start_sized(width);
        let full = s.dir().join(original.file_name().expect("a file name"));
        std::fs::copy(&original, &full).unwrap_or_else(|e| panic!("{path}: {e}"));
        let loaded = crate::read_file(&full).unwrap_or_else(|e| panic!("{path}: {e}"));
        let name: SharedString = full.file_name().unwrap().to_string_lossy().into_owned().into();
        let is_tmpl = full.extension().and_then(|e| e.to_str()).is_some_and(ibis_pptx::is_template);
        let is_old_presentation = full.extension().and_then(|e| e.to_str()).is_some_and(|x| ["ppt", "pps", "pot", "odp", "otp"].iter().any(|o| x.eq_ignore_ascii_case(o)));
        s.update(|e, window, cx| {
            if is_tmpl {
                let tmpl_name = crate::shell::file_actions::new_file_name(ibis_pptx::Kind::of(&loaded.presentation));
                e.load(loaded.presentation, tmpl_name, window, cx);
                e.source = Some(loaded.source);
                e.path = None;
            } else if is_old_presentation {
                // An old presentation is read as a new .pptx: saving asks where.
                let stem = full.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                e.load(loaded.presentation, format!("{stem}.pptx").into(), window, cx);
                e.source = Some(loaded.source);
                e.path = None;
            } else {
                e.load(loaded.presentation, name, window, cx);
                e.source = Some(loaded.source);
                e.set_file_path(Some(full.clone()));
            }
            if full.extension().and_then(|e| e.to_str()).is_some_and(ibis_pptx::is_slideshow) {
                e.start_slideshow(window, cx);
            }
        });
        s.settle();
        s
    }

    /// A window with a new, empty document of `kind` (not saveable yet).
    #[allow(dead_code)] // For the formats' scenarios.
    pub(crate) fn new_document(kind: ibis_pptx::Kind) -> Self {
        let mut s = Self::start();
        s.update(|e, window, cx| e.load(ibis_pptx::blank(kind), "新規".into(), window, cx));
        s.settle();
        s
    }

    /// A window showing the start view (no document), as the application
    /// opens.
    #[allow(dead_code)] // For the formats' scenarios.
    pub(crate) fn start_view() -> Self {
        let mut s = Self::start();
        s.settle();
        s
    }

    fn start() -> Self {
        Self::start_sized(1200.0)
    }

    fn start_sized(width: f32) -> Self {
        let lock = ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // The scenarios read the UI in Japanese, whatever the system's
        // language and the user's setting.
        crate::shell::i18n::show(crate::shell::i18n::Language::Japanese);
        let name = CURRENT.with(|c| c.borrow().clone());
        let name = if name.is_empty() { "scenario".to_string() } else { name };
        // One text system for all the scenarios of the process: a new one for each (its fonts and glyph caches)
        // stayed in memory after the scenario, about 55 MB each, 5 GB in 90 scenarios.
        thread_local! {
            static TEXT_SYSTEM: Arc<dyn gpui_kit::PlatformTextSystem> =
                Arc::new(drawn_text::Logging { inner: gpui_kit::platform::current_platform(true).text_system() });
        }
        let text_system = TEXT_SYSTEM.with(|t| t.clone());
        let mut cx = HeadlessAppContext::with_platform(text_system, Arc::new(crate::shell::icons::Assets::new("")), crate::export::headless_renderer);
        cx.allow_parking();
        cx.update(crate::init);
        let slot: std::rc::Rc<std::cell::RefCell<Option<Entity<SlideEditor>>>> = Default::default();
        let set = slot.clone();
        let window = cx
            .open_window(size(px(width), px(720.)), move |window, app| {
                let view = app.new(|cx| SlideEditor::new(ibis_doc_model::Presentation::new(), window, cx));
                let focus = view.read(app).focus_handle.clone();
                focus.focus(window, app);
                *set.borrow_mut() = Some(view.clone());
                app.new(|cx| Root::new(view, window, cx))
            })
            .expect("a headless window");
        let editor = slot.borrow().clone().expect("the editor");
        Self { editor, window, cx, name, shots: 0, _lock: lock }
    }

    /// The app ends with its last window, as `run_native` makes it (here the request is recorded: `quit_requested`).
    #[allow(dead_code)] // For the formats' scenarios.
    pub(crate) fn quit_with_last_window(&mut self) {
        crate::QUIT_REQUESTED.store(false, std::sync::atomic::Ordering::Relaxed);
        self.cx.update(|app| app.on_window_closed(|cx, _| crate::quit_if_last_window_closed(cx)).detach());
    }

    /// The app asked to quit (after `quit_with_last_window`).
    #[allow(dead_code)] // For the formats' scenarios.
    pub(crate) fn quit_requested(&self) -> bool {
        crate::QUIT_REQUESTED.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// How many windows the app has open.
    #[allow(dead_code)] // For the formats' scenarios.
    pub(crate) fn window_count(&mut self) -> usize {
        self.cx.update(|app| app.windows().len())
    }

    /// Press the window's close button (the red button on a Mac, × elsewhere), as the platform does: the window goes if
    /// the editor lets it (`window_should_close`, registered by `open_window_sized`). Whether it closed.
    #[allow(dead_code)] // For the formats' scenarios.
    pub(crate) fn press_close_button(&mut self) -> bool {
        let editor = self.editor.clone();
        let close = self.cx.update(|app| editor.update(app, |e, cx| e.window_should_close(cx)));
        if close {
            self.cx.update_window(self.window.into(), |_, w, _| w.remove_window()).expect("the window");
        }
        self.cx.run_until_parked();
        close
    }

    /// Let everything run and draw the window (a few frames, pictures decoded).
    pub(crate) fn settle(&mut self) {
        let preload = self.cx.update(|app| self.editor.read(app).resources.preload.clone());
        preload.wait_all();
        for _ in 0..3 {
            self.cx.run_until_parked();
            self.cx.update_window(self.window.into(), |_, window, _| window.refresh()).ok();
        }
    }

    /// Let `ms` milliseconds pass on the scenario's clock (it is simulated:
    /// timers fire when it is advanced, not when real time passes), then
    /// settle.
    #[allow(dead_code)] // For the formats' scenarios.
    pub(crate) fn wait(&mut self, ms: u64) {
        self.cx.advance_clock(std::time::Duration::from_millis(ms));
        self.settle();
    }

    /// Run `f` on the editor, with its window and context, then settle.
    pub(crate) fn update<R>(&mut self, f: impl FnOnce(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) -> R) -> R {
        let editor = self.editor.clone();
        let r = self.cx.update_window(self.window.into(), |_, window, app| editor.update(app, |e, cx| f(e, window, cx))).expect("the window");
        self.settle();
        r
    }

    /// Read the editor (its model: `e.presentation()`, its state).
    pub(crate) fn with<R>(&mut self, f: impl FnOnce(&SlideEditor, &App) -> R) -> R {
        let editor = self.editor.clone();
        self.cx.update(|app| f(editor.read(app), app))
    }

    /// Run `f` with mutable access to the app context.
    pub(crate) fn with_app<R>(&mut self, f: impl FnOnce(&mut App) -> R) -> R {
        self.cx.update(f)
    }

    /// Resize the window's content.
    pub(crate) fn size(&mut self, width: f32, height: f32) {
        self.cx
            .update_window(self.window.into(), |_, window, app| {
                window.resize(size(px(width), px(height)));
                window.bounds_changed(app);
            })
            .ok();
        self.settle();
    }

    pub(crate) fn dispatch(&mut self, events: Vec<PlatformInput>) {
        self.cx
            .update_window(self.window.into(), |_, window, app| {
                for event in events {
                    window.dispatch_event(event, app);
                }
            })
            .ok();
        self.settle();
    }

    fn press(position: Point<Pixels>, clicks: usize) -> Vec<PlatformInput> {
        vec![
            PlatformInput::MouseMove(MouseMoveEvent { position, ..Default::default() }),
            PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position, click_count: clicks, ..Default::default() }),
            PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position, click_count: clicks, ..Default::default() }),
        ]
    }

    /// A click at window position (`x`, `y`) in pixels.
    pub(crate) fn click(&mut self, (x, y): (f32, f32)) {
        self.dispatch(Self::press(point(px(x), px(y)), 1));
    }

    /// A pinch-to-zoom gesture on a trackpad at `(x, y)` with magnification `delta`.
    pub(crate) fn pinch(&mut self, (x, y): (f32, f32), delta: f32) {
        let position = point(px(x), px(y));
        self.dispatch(vec![PlatformInput::Pinch(PinchEvent {
            position,
            delta,
            modifiers: Modifiers::default(),
            phase: TouchPhase::Moved,
        })]);
    }

    /// A click with the right button (opens a context menu).
    pub(crate) fn right_click(&mut self, (x, y): (f32, f32)) {
        let position = point(px(x), px(y));
        self.dispatch(vec![
            PlatformInput::MouseMove(MouseMoveEvent { position, ..Default::default() }),
            PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Right, position, click_count: 1, ..Default::default() }),
            PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Right, position, click_count: 1, ..Default::default() }),
        ]);
    }

    /// A click with `modifiers` held (⌘-click: `platform`).
    pub(crate) fn dispatch_click_with(&mut self, (x, y): (f32, f32), modifiers: Modifiers) {
        let position = point(px(x), px(y));
        self.dispatch(vec![
            PlatformInput::MouseMove(MouseMoveEvent { position, modifiers, ..Default::default() }),
            PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position, click_count: 1, modifiers, ..Default::default() }),
            PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position, click_count: 1, modifiers, ..Default::default() }),
        ]);
    }

    /// A click with Shift held.
    pub(crate) fn shift_click(&mut self, (x, y): (f32, f32)) {
        let position = point(px(x), px(y));
        let modifiers = Modifiers { shift: true, ..Default::default() };
        self.dispatch(vec![
            PlatformInput::MouseMove(MouseMoveEvent { position, modifiers, ..Default::default() }),
            PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position, click_count: 1, modifiers, ..Default::default() }),
            PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position, click_count: 1, modifiers, ..Default::default() }),
        ]);
    }

    /// A double click (starts editing a shape's text).
    pub(crate) fn double_click(&mut self, (x, y): (f32, f32)) {
        let at = point(px(x), px(y));
        let mut events = Self::press(at, 1);
        events.extend(Self::press(at, 2));
        self.dispatch(events);
    }

    /// A triple click (selects a paragraph in docx).
    pub(crate) fn triple_click(&mut self, (x, y): (f32, f32)) {
        let at = point(px(x), px(y));
        let mut events = Self::press(at, 1);
        events.extend(Self::press(at, 2));
        events.extend(Self::press(at, 3));
        self.dispatch(events);
    }

    /// Press at `from`, move to `to` and release.
    pub(crate) fn drag(&mut self, (x0, y0): (f32, f32), (x1, y1): (f32, f32)) {
        self.drag_with((x0, y0), (x1, y1), Modifiers::default());
    }

    /// Files dragged from the Finder and dropped at `at`.
    pub(crate) fn drop_files(&mut self, (x, y): (f32, f32), paths: Vec<PathBuf>) {
        let position = point(px(x), px(y));
        self.dispatch(vec![
            PlatformInput::FileDrop(FileDropEvent::Entered { position, paths: ExternalPaths(paths.into_iter().collect()) }),
            PlatformInput::FileDrop(FileDropEvent::Pending { position }),
            PlatformInput::FileDrop(FileDropEvent::Submit { position }),
        ]);
    }

    /// A drag with `modifiers` held (Option, Shift…).
    pub(crate) fn drag_with(&mut self, (x0, y0): (f32, f32), (x1, y1): (f32, f32), modifiers: Modifiers) {
        let (from, to) = (point(px(x0), px(y0)), point(px(x1), px(y1)));
        self.dispatch(vec![
            PlatformInput::MouseMove(MouseMoveEvent { position: from, modifiers, ..Default::default() }),
            PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: from, click_count: 1, modifiers, ..Default::default() }),
            PlatformInput::MouseMove(MouseMoveEvent { position: to, pressed_button: Some(MouseButton::Left), modifiers, ..Default::default() }),
            PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: to, click_count: 1, modifiers, ..Default::default() }),
        ]);
    }

    /// Press at the first point, move through the others (the button down) and
    /// release at the last, as a hand drags, one move at a time.
    pub(crate) fn drag_through(&mut self, path: &[(f32, f32)]) {
        let at = |(x, y): (f32, f32)| point(px(x), px(y));
        let mut events = vec![
            PlatformInput::MouseMove(MouseMoveEvent { position: at(path[0]), ..Default::default() }),
            PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: at(path[0]), click_count: 1, ..Default::default() }),
        ];
        for &p in &path[1..] {
            events.push(PlatformInput::MouseMove(MouseMoveEvent { position: at(p), pressed_button: Some(MouseButton::Left), ..Default::default() }));
        }
        let last = at(*path.last().expect("a path"));
        events.push(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: last, click_count: 1, ..Default::default() }));
        self.dispatch(events);
    }

    /// Where the middle of shape `index` of the current slide is, in the window.
    pub(crate) fn shape_position(&mut self, index: usize) -> (f32, f32) {
        self.with(|e, _| {
            let shape = &e.slide().shapes[index];
            let layout = e.slide_layout().expect("the slide is laid out");
            let f = shape.frame;
            let x = layout.origin.x + layout.to_px(f.x + f.width / 2);
            let y = layout.origin.y + layout.to_px(f.y + f.height / 2);
            (f32::from(x), f32::from(y))
        })
    }

    /// A click on shape `index` of the current slide.
    pub(crate) fn click_shape(&mut self, index: usize) {
        let at = self.shape_position(index);
        self.click(at);
    }

    /// Select shape `index` of the current slide as PowerPoint does: a click inside a shape
    /// that has text puts the caret in it, so Escape then leaves the shape selected.
    pub(crate) fn select_shape(&mut self, index: usize) {
        self.click_shape(index);
        if self.with(|e, _| e.text_editing().is_some()) {
            self.key("escape");
        }
    }

    /// A double click on shape `index` of the current slide (edits its text).
    pub(crate) fn double_click_shape(&mut self, index: usize) {
        let at = self.shape_position(index);
        self.double_click(at);
    }

    /// A key, as GPUI writes it: `"cmd-b"`, `"enter"`, `"shift-tab"`.
    pub(crate) fn key(&mut self, keystroke: &str) {
        let keystroke = Keystroke::parse(keystroke).unwrap_or_else(|e| panic!("{keystroke}: {e:?}"));
        self.cx.update_window(self.window.into(), |_, window, app| window.dispatch_keystroke(keystroke, app)).ok();
        self.settle();
    }

    /// Type `text` a character at a time, as the keyboard does.
    pub(crate) fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            let key = c.to_lowercase().to_string();
            let keystroke = Keystroke { modifiers: Modifiers { shift: c.is_uppercase(), ..Default::default() }, key, key_char: Some(c.to_string()) };
            self.cx.update_window(self.window.into(), |_, window, app| window.dispatch_keystroke(keystroke, app)).ok();
        }
        self.settle();
    }

    /// The input method: `composing` shown (marked) at the caret, then
    /// `commit` put in, through the format's input hooks (the path the
    /// system's input method takes, `shell/input.rs`).
    pub(crate) fn ime(&mut self, composing: &str, commit: &str) {
        let n = composing.encode_utf16().count();
        self.update(|e, window, cx| {
            assert!(with_format!(e, wants_text_input(e, window, cx)), "the format takes no text input now");
            with_format!(e, replace_and_mark_text_in_range(e, None, composing, Some(n..n), window, cx));
        });
        self.update(|e, window, cx| with_format!(e, replace_text_in_range(e, None, commit, window, cx)));
    }

    /// The toolbar's part with id `id` (as the format registered it).
    fn tool(&mut self, id: &str) -> Option<ToolItem> {
        self.with(|e, _| {
            e.toolbar_items().into_iter().find(|item| match item {
                ToolItem::Button(b) => b.id == id,
                ToolItem::Select { id: i, .. } => *i == id,
                _ => false,
            })
        })
    }

    /// Toolbar button `id` as it is now (its icon, whether it is a split button, ...).
    #[allow(dead_code)] // For the formats' scenarios.
    pub(crate) fn tool_button(&mut self, id: &str) -> Option<crate::shell::chrome::ToolButton> {
        match self.tool(id)? {
            ToolItem::Button(b) => Some(b),
            _ => None,
        }
    }

    /// What toolbar button `id` shows, or `None` if the toolbar has none.
    pub(crate) fn toolbar_state(&mut self, id: &str) -> Option<ToolState> {
        match self.tool(id)? {
            ToolItem::Button(b) => Some(ToolState { on: b.on, enabled: b.enabled() }),
            ToolItem::Select { disabled, menu, .. } => Some(ToolState { on: false, enabled: !disabled && !menu.is_empty() }),
            _ => None,
        }
    }

    /// Press toolbar button `id` (it must be there and enabled). A button
    /// that opens a menu: use [`toolbar_menu`](Self::toolbar_menu).
    pub(crate) fn toolbar(&mut self, id: &str) {
        let Some(ToolItem::Button(b)) = self.tool(id) else { panic!("no toolbar button {id}") };
        assert!(b.enabled(), "toolbar button {id} is disabled");
        let handler = b.left_action().cloned().or(b.handler).unwrap_or_else(|| panic!("toolbar button {id} opens a menu; use toolbar_menu"));
        self.update(|e, window, cx| handler(e, window, cx));
    }

    /// Choose item `item` of the menu of toolbar button (or box) `id`.
    pub(crate) fn toolbar_menu(&mut self, id: &str, item: &str) {
        let menu = match self.tool(id) {
            Some(ToolItem::Button(b)) => b.menu.unwrap_or_default(),
            Some(ToolItem::Select { menu, .. }) => menu,
            _ => panic!("no toolbar button {id}"),
        };
        let handler = find_item(&menu, item).and_then(|i| i.enabled().then_some(i.handler.clone()).flatten()).unwrap_or_else(|| panic!("{id} has no enabled item {item}"));
        self.update(|e, window, cx| handler(e, window, cx));
    }

    /// Whether menu item `id` is enabled and checked, or `None` if no menu has it.
    pub(crate) fn menu_state(&mut self, id: &str) -> Option<(bool, bool)> {
        self.with(|e, _| {
            let mut menus = e.menus();
            menus.find_mut(id).map(|i| (i.enabled(), i.checked))
        })
    }

    /// Choose menu item `id` (it must be enabled).
    pub(crate) fn menu(&mut self, id: &str) {
        let handler = self.with(|e, _| {
            let mut menus = e.menus();
            let item = menus.find_mut(id).unwrap_or_else(|| panic!("no menu item {id}"));
            assert!(item.enabled(), "menu item {id} is disabled");
            item.handler.clone().expect("an enabled item has a handler")
        });
        self.update(|e, window, cx| handler(e, window, cx));
    }

    /// Show page (slide, sheet) `number`, 1-based.
    pub(crate) fn page(&mut self, number: usize) {
        self.update(|e, window, cx| e.show_slide_number(number, window, cx));
    }

    /// The status bar's texts (page, the format's items, the hint).
    pub(crate) fn status_texts(&mut self) -> Vec<String> {
        self.with(|e, _| e.status_texts().into_iter().map(|t| t.to_string()).collect())
    }

    /// Save to `path` (under `target/scenario/<test name>/` if relative), as
    /// 名前を付けて保存 does. Panics if saving fails.
    pub(crate) fn save_as(&mut self, path: &str) -> PathBuf {
        let path = if Path::new(path).is_absolute() { PathBuf::from(path) } else { self.dir().join(path) };
        let target = path.clone();
        self.update(|e, window, _| e.save_to(target, window)).unwrap_or_else(|err| panic!("saving failed: {err}"));
        path
    }

    /// Replace the live editor document and source with its saved file.
    /// This also resets history through the ordinary file loading path.
    pub(crate) fn reload_saved(&mut self) {
        let path = self.with(|e, _| e.path.clone()).expect("the file has a path");
        let loaded = crate::read_file(&path).unwrap();
        let name: SharedString = path.file_name().unwrap().to_string_lossy().into_owned().into();
        self.update(|e, window, cx| {
            e.load(loaded.presentation, name, window, cx);
            e.source = Some(loaded.source);
            e.set_file_path(Some(path));
        });
        self.settle();
    }

    /// The saved file (the file's path now) read again, as a new window would.
    pub(crate) fn reopen(&mut self) -> ibis_doc_model::Presentation {
        let path = self.with(|e, _| e.path.clone()).expect("the file has a path");
        let data = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        ibis_pptx::read(&data).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    fn dir(&self) -> PathBuf {
        let dir = repo_root().join("target/scenario").join(&self.name);
        std::fs::create_dir_all(&dir).expect("the scenario's folder");
        dir
    }

    /// A picture of the window, as `target/scenario/<test>/<nn>-<label>.png`.
    /// The pointer moved to window position (`x`, `y`), no button down.
    #[allow(dead_code)] // For the scenarios.
    pub(crate) fn hover(&mut self, (x, y): (f32, f32)) {
        let position = point(px(x), px(y));
        self.cx
            .update_window(self.window.into(), |_, window, app| {
                window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position, ..Default::default() }), app);
            })
            .ok();
        self.settle();
    }

    /// The mouse wheel at window position (`x`, `y`): `dy` pixels, positive
    /// scrolling up (the content moving down), as a trackpad sends.
    #[allow(dead_code)] // For the formats' scenarios.
    pub(crate) fn wheel(&mut self, at: (f32, f32), dy: f32) {
        self.wheel_xy(at, 0.0, dy);
    }

    /// `wheel` with a sideways amount `dx` too (positive scrolling left).
    pub(crate) fn wheel_xy(&mut self, (x, y): (f32, f32), dx: f32, dy: f32) {
        // Drawn as the window draws after an event: once, for the views
        // the event changed (not every view again, as `settle` does).
        let position = point(px(x), px(y));
        self.cx
            .update_window(self.window.into(), |_, window, app| {
                window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position, ..Default::default() }), app);
                window.dispatch_event(PlatformInput::ScrollWheel(ScrollWheelEvent { position, delta: ScrollDelta::Pixels(point(px(dx), px(dy))), ..Default::default() }), app);
            })
            .ok();
        self.cx.run_until_parked();
    }

    /// The mouse wheel with the zoom modifier (`⌃⌘` on macOS) at window
    /// position (`x`, `y`): `lines` notches (negative zooming out).
    #[allow(dead_code)]
    pub(crate) fn wheel_zoom_lines(&mut self, (x, y): (f32, f32), lines: f32) {
        let position = point(px(x), px(y));
        let modifiers = Modifiers { platform: true, control: true, ..Default::default() };
        self.cx
            .update_window(self.window.into(), |_, window, app| {
                window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position, modifiers, ..Default::default() }), app);
                window.dispatch_event(PlatformInput::ScrollWheel(ScrollWheelEvent { position, delta: ScrollDelta::Lines(point(0.0, lines)), modifiers, ..Default::default() }), app);
            })
            .ok();
        self.cx.run_until_parked();
    }

    /// Advance the simulated clock by `ms` milliseconds without settling
    /// extra frames (for 60fps 16ms event pacing).
    #[allow(dead_code)]
    pub(crate) fn advance_ms(&mut self, ms: u64) {
        self.cx.advance_clock(std::time::Duration::from_millis(ms));
        self.cx.run_until_parked();
    }

    /// The window drawn now, without saving it: its width in pixels and
    /// its pixels (RGBA, row by row).
    #[allow(dead_code)] // For the formats' scenarios.
    pub(crate) fn picture(&mut self) -> (u32, Vec<u8>) {
        let image = self.cx.capture_screenshot(self.window.into()).expect("a screenshot");
        (image.width(), image.into_raw())
    }

    /// Run `f` on `view`, an editor in another window the app opened
    /// (`window`), then settle.
    pub(crate) fn update_in<R>(&mut self, window: AnyWindowHandle, view: &Entity<SlideEditor>, f: impl FnOnce(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) -> R) -> R {
        let r = self.cx.update_window(window, |_, w, app| view.update(app, |e, cx| f(e, w, cx))).expect("the window");
        self.settle();
        r
    }

    /// Run `f` in another window the app opened.
    pub(crate) fn update_window<R>(&mut self, window: AnyWindowHandle, f: impl FnOnce(&mut Window, &mut App) -> R) -> R {
        self.cx.update_window(window, |_, w, app| f(w, app)).expect("the window")
    }

    /// Close another window the app opened, then settle.
    pub(crate) fn close_window(&mut self, window: AnyWindowHandle) {
        self.cx.update_window(window, |_, w, _| w.remove_window()).expect("the window");
        self.settle();
    }

    /// A screenshot of another window the app opened.
    pub(crate) fn screenshot_of(&mut self, window: AnyWindowHandle, label: &str) -> PathBuf {
        self.shots += 1;
        let path = self.dir().join(format!("{:02}-{label}.png", self.shots));
        let image = self.cx.capture_screenshot(window).expect("a screenshot");
        image.save(&path).expect("saving the screenshot");
        path
    }

    /// Where a picture named `label` is saved among this scenario's screenshots (no number put before it).
    #[allow(dead_code)] // For the formats' scenarios.
    pub(crate) fn screenshot_path(&mut self, label: &str) -> PathBuf {
        self.dir().join(format!("{label}.png"))
    }

    pub(crate) fn screenshot(&mut self, label: &str) -> PathBuf {
        self.shots += 1;
        let path = self.dir().join(format!("{:02}-{label}.png", self.shots));
        let image = self.cx.capture_screenshot(self.window.into()).expect("a screenshot");
        image.save(&path).expect("saving the screenshot");
        path
    }
}

pub(crate) fn find_item<'a>(entries: &'a [MenuEntry], id: &str) -> Option<&'a crate::shell::chrome::MenuItem> {
    entries.iter().find_map(|entry| match entry {
        MenuEntry::Item(item) if item.id == id => Some(item),
        MenuEntry::Submenu(sub) => find_item(&sub.items, id),
        MenuEntry::Palette(p) => p.find(id),
        _ => None,
    })
}

/// What the scenarios' windows laid out as text: the text system the
/// headless windows use records every line it is asked to lay out, to find
/// the ids of the UI tables drawn as they are (an untranslated label).
mod drawn_text {
    use std::borrow::Cow;
    use std::collections::BTreeSet;
    use std::sync::{Arc, Mutex};

    use gpui_kit::{Bounds, DevicePixels, Font, FontId, FontMetrics, FontRun, GlyphId, Hsla, LineLayout, PlatformTextSystem, Pixels, RenderGlyphParams, Size, TextRenderingMode};

    static DRAWN: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

    pub(super) struct Logging {
        pub(super) inner: Arc<dyn PlatformTextSystem>,
    }

    impl PlatformTextSystem for Logging {
        fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> gpui_kit::Result<()> {
            self.inner.add_fonts(fonts)
        }
        fn all_font_names(&self) -> Vec<String> {
            self.inner.all_font_names()
        }
        fn font_id(&self, descriptor: &Font) -> gpui_kit::Result<FontId> {
            self.inner.font_id(descriptor)
        }
        fn font_metrics(&self, font_id: FontId) -> FontMetrics {
            self.inner.font_metrics(font_id)
        }
        fn font_data(&self, font_id: FontId) -> Option<(Arc<Vec<u8>>, u32)> {
            self.inner.font_data(font_id)
        }
        fn synthetic_style(&self, font_id: FontId) -> (bool, bool) {
            self.inner.synthetic_style(font_id)
        }
        fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> gpui_kit::Result<Bounds<f32>> {
            self.inner.typographic_bounds(font_id, glyph_id)
        }
        fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> gpui_kit::Result<Size<f32>> {
            self.inner.advance(font_id, glyph_id)
        }
        fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
            self.inner.glyph_for_char(font_id, ch)
        }
        fn glyph_raster_bounds(&self, params: &RenderGlyphParams) -> gpui_kit::Result<Bounds<DevicePixels>> {
            self.inner.glyph_raster_bounds(params)
        }
        fn rasterize_glyph(&self, params: &RenderGlyphParams, raster_bounds: Bounds<DevicePixels>) -> gpui_kit::Result<(Size<DevicePixels>, Vec<u8>)> {
            self.inner.rasterize_glyph(params, raster_bounds)
        }
        fn layout_line(&self, text: &str, font_size: Pixels, runs: &[FontRun]) -> LineLayout {
            if text.len() < 160 && text.contains('.')
                && let Ok(mut drawn) = DRAWN.lock()
            {
                drawn.insert(text.to_string());
            }
            self.inner.layout_line(text, font_size, runs)
        }
        fn recommended_rendering_mode(&self, font_id: FontId, font_size: Pixels) -> TextRenderingMode {
            self.inner.recommended_rendering_mode(font_id, font_size)
        }
        fn glyph_dilation_for_color(&self, color: Hsla) -> u8 {
            self.inner.glyph_dilation_for_color(color)
        }
    }

    /// The drawn strings that are ids of the tables.
    pub(super) fn ids_drawn() -> Vec<String> {
        DRAWN
            .lock()
            .map(|drawn| drawn.iter().filter(|s| crate::shell::i18n::is_known_id(s)).cloned().collect())
            .unwrap_or_default()
    }
}
