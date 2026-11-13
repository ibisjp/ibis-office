// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the UI's language (#8): ツール > 設定… opens the settings
//! window; choosing English there turns the menus English at once; the
//! choice is kept and read again at the next start.
use crate::shell::chrome::MenuId;
use crate::shell::i18n::{self, Language, LanguageSetting};
use crate::shell::scenario::Scenario;
use crate::docx::ext::*;
use crate::xlsx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("shell::i8_language_switch_and_kept", i8_language_switch_and_kept), ("shell::i8_panels_and_status_in_english", i8_panels_and_status_in_english), ("shell::fix16_font_names_in_english", fix16_font_names_in_english), ("shell::i8_docx_panels_in_english", i8_docx_panels_in_english), ("shell::fix34_start_view_in_english", fix34_start_view_in_english), ("shell::fix34_new_file_names_in_english", fix34_new_file_names_in_english), ("shell::fix36_font_lists_from_the_system", fix36_font_lists_from_the_system), ("shell::split_buttons_in_english", split_buttons_in_english), ("shell::fix_font_dialog_item_leads_the_list", fix_font_dialog_item_leads_the_list), ("shell::replace_fonts_document_and_workbook", replace_fonts_document_and_workbook)]
}

/// The label of the menu item `id` and of the ファイル menu, as shown now.
fn labels(s: &mut Scenario, id: &str) -> (String, String) {
    let id = id.to_string();
    s.with(move |e, _| {
        let mut menus = e.menus();
        (menus.find_mut(&id).map(|i| i.label.to_string()).unwrap_or_default(), MenuId::File.label().to_string())
    })
}

fn i8_language_switch_and_kept() {
    let mut s = Scenario::new_document(ibis_pptx::Kind::Workbook);
    // The settings are kept in the scenario's folder, not the user's.
    let dir = crate::shell::scenario::repo_root().join("target/scenario/shell::i8_language_switch_and_kept/settings");
    let _ = std::fs::remove_dir_all(&dir);
    // SAFETY: the scenarios run one at a time on the main thread; nothing
    // else reads the environment meanwhile.
    unsafe { std::env::set_var("IBIS_SETTINGS_DIR", &dir) };
    assert_eq!(labels(&mut s, "file.save"), ("保存".into(), "ファイル".into()));
    assert_eq!(labels(&mut s, "xlsx.remove-duplicates").0, "重複の削除…");
    // ツール > 設定… opens the settings dialog.
    s.menu("tools.settings");
    assert!(s.with(|e, _| e.settings.is_some()));
    // English, at once.
    s.update(|_, _, cx| crate::shell::settings_dialog::choose_language(LanguageSetting::Fixed(Language::English), cx));
    assert_eq!(labels(&mut s, "file.save"), ("Save".into(), "File".into()));
    assert_eq!(labels(&mut s, "tools.settings").0, "Settings…");
    assert_eq!(labels(&mut s, "xlsx.remove-duplicates").0, "Remove Duplicates…");
    s.screenshot("english-menus");
    // Kept: read again as at the next start.
    i18n::forget();
    assert_eq!(i18n::setting(), LanguageSetting::Fixed(Language::English));
    assert_eq!(i18n::language(), Language::English);
    assert!(std::fs::read_to_string(dir.join("settings.txt")).unwrap_or_default().contains("language=en"));
    // Back to Japanese; the system's language when chosen so.
    s.update(|_, _, cx| crate::shell::settings_dialog::choose_language(LanguageSetting::Fixed(Language::Japanese), cx));
    assert_eq!(labels(&mut s, "file.save").0, "保存");
    s.update(|_, _, cx| crate::shell::settings_dialog::choose_language(LanguageSetting::System, cx));
    assert_eq!(i18n::language(), i18n::system_language());
    unsafe { std::env::remove_var("IBIS_SETTINGS_DIR") };
    i18n::show(Language::Japanese);
}

/// In English, the status bar (シート → Sheet), a panel's title and its
/// buttons, the toolbar's tooltips: drawn from the tables by their
/// Japanese text (a picture to look at).
fn i8_panels_and_status_in_english() {
    let mut s = Scenario::new_document(ibis_pptx::Kind::Workbook);
    i18n::show(Language::English);
    s.update(|_, _, _| {});
    let status = s.status_texts();
    assert!(status[0].starts_with("Sheet 1"), "{status:?}");
    s.menu("xlsx.remove-duplicates");
    s.screenshot("english-panel");
    i18n::show(Language::Japanese);
    s.update(|_, _, _| {});
    assert!(s.status_texts()[0].starts_with("シート 1"));
}

/// #16: the English UI's font lists show the English name of a font that has
/// one (a picture: the font box of a new document), the Japanese UI and the
/// document keep the Japanese name.
fn fix16_font_names_in_english() {
    let mut s = Scenario::new_document(ibis_pptx::Kind::Document);
    i18n::show(Language::English);
    assert_eq!(i18n::font_label("游明朝"), "Yu Mincho");
    assert_eq!(i18n::font_label("ＭＳ ゴシック"), "MS Gothic");
    assert_eq!(i18n::font_label("Arial"), "Arial");
    s.update(|_, _, _| {});
    s.screenshot("english-font-box");
    i18n::show(Language::Japanese);
    assert_eq!(i18n::font_label("游明朝"), "游明朝");
    assert_eq!(i18n::font_label("BIZ UDPゴシック"), "BIZ UDPゴシック");
}

/// A document's page setup panel and watermark panel in English (pictures to
/// look at): their labels are drawn from the tables by id.
fn i8_docx_panels_in_english() {
    let mut s = Scenario::new_document(ibis_pptx::Kind::Document);
    i18n::show(Language::English);
    s.update(|e, window, cx| e.open_page_setup_panel(window, cx));
    s.screenshot("english-page-setup");
    s.update(|e, window, cx| e.open_watermark_panel(window, cx));
    s.screenshot("english-watermark");
    i18n::show(Language::Japanese);
}

/// #34: the start view in English (a picture to look at).
fn fix34_start_view_in_english() {
    let mut s = Scenario::start_view();
    i18n::show(Language::English);
    s.update(|_, _, _| {});
    s.screenshot("english-start");
    i18n::show(Language::Japanese);
    s.update(|_, _, _| {});
}

/// #34: the names a new file shows until it is saved follow the UI's language
/// (Word's Document1, PowerPoint's Presentation1; Japanese as before).
fn fix34_new_file_names_in_english() {
    use ibis_pptx::Kind;
    i18n::show(Language::English);
    assert_eq!(crate::shell::file_actions::new_file_name(Kind::Document), "Document1.docx");
    assert_eq!(crate::shell::file_actions::new_file_name(Kind::Presentation), "Presentation1.pptx");
    i18n::show(Language::Japanese);
    assert_eq!(crate::shell::file_actions::new_file_name(Kind::Document), "文書 1.docx");
    assert_eq!(crate::shell::file_actions::new_file_name(Kind::Presentation), "プレゼンテーション 1.pptx");
}

/// The split buttons (borders, underline, merge, Σ, bullets, numbering, new slide, table borders) in English: their
/// tooltips and the labels of their menus, as the menus draw them, have no Japanese and no id left as it is; a picture
/// of the toolbar.
fn split_buttons_in_english() {
    use crate::shell::chrome::MenuEntry;
    fn labels(entries: &[MenuEntry], out: &mut Vec<String>) {
        for entry in entries {
            match entry {
                MenuEntry::Item(item) if item.id != "pptx.new-slide.layout" => out.push(i18n::text(&item.label).to_string()),
                MenuEntry::Submenu(sub) => {
                    out.push(i18n::text(&sub.label).to_string());
                    labels(&sub.items, out);
                }
                _ => {}
            }
        }
    }
    let japanese = |t: &str| t.chars().any(|c| matches!(c, '\u{3040}'..='\u{30ff}' | '\u{4e00}'..='\u{9fff}'));
    let check = |s: &mut Scenario, ids: &[&str]| {
        for id in ids {
            let b = s.tool_button(id).unwrap_or_else(|| panic!("no button {id}"));
            assert!(b.left_action().is_some() && b.menu.is_some(), "{id} is a split button");
            let mut texts = vec![i18n::text(&b.tooltip).to_string()];
            labels(b.menu.as_deref().unwrap_or_default(), &mut texts);
            for t in texts {
                assert!(!japanese(&t), "{id}: {t:?} is Japanese in the English UI");
                assert!(!t.is_empty() && !(t.contains('.') && !t.contains(' ') && t.chars().all(|c| c.is_ascii_lowercase() || c == '.' || c == '-' || c.is_ascii_digit() || c == '_')), "{id}: {t:?} is an id drawn as it is");
            }
        }
    };
    let mut s = Scenario::new_document(ibis_pptx::Kind::Workbook);
    i18n::show(Language::English);
    s.update(|_, _, _| {});
    check(&mut s, &["borders", "underline", "merge", "functions", "currency"]);
    s.screenshot("english-xlsx-toolbar");
    drop(s);
    let mut s = Scenario::new_document(ibis_pptx::Kind::Document);
    i18n::show(Language::English);
    s.update(|_, _, _| {});
    check(&mut s, &["bullets", "numbered", "underline"]);
    s.screenshot("english-docx-toolbar");
    drop(s);
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    i18n::show(Language::English);
    s.page(2);
    s.double_click_shape(1);
    s.key("cmd-a");
    check(&mut s, &["new-slide", "bullets", "numbered", "underline"]);
    s.screenshot("english-pptx-toolbar");
    s.key("escape");
    s.page(3);
    s.menu("table.new.3x3");
    check(&mut s, &["table.borders"]);
    i18n::show(Language::Japanese);
    s.update(|_, _, _| {});
}

/// Select one shape with text on the slide shown (a presentation's font box is there for it).
fn select_a_text_shape(e: &mut crate::shell::editor::SlideEditor, window: &mut gpui_kit::Window, cx: &mut gpui_kit::Context<crate::shell::editor::SlideEditor>) {
    let slide = e.slide();
    let id = slide.shown_shapes().find(|s| s.text_body().is_some_and(|b| !b.text().is_empty())).map(|s| s.id).expect("a shape with text");
    e.end_text_edit(window, cx);
    e.set_selection(vec![id]);
    cx.notify();
}

/// The font box of a document: (the value shown, the labels of the list, the checked ones).
fn font_box(s: &mut Scenario) -> (String, Vec<String>, Vec<String>) {
    use crate::shell::chrome::{MenuEntry, ToolItem};
    s.with(|e, _| {
        let Some(ToolItem::Select { value, menu, .. }) = e.toolbar_items().into_iter().find(|i| matches!(i, ToolItem::Select { id: "font", .. })) else { panic!("no font box") };
        let items: Vec<_> = menu.iter().filter_map(|m| if let MenuEntry::Item(i) = m { Some(i) } else { None }).filter(|i| i.id.ends_with(".font")).collect();
        (value.to_string(), items.iter().map(|i| i.label.to_string()).collect(), items.iter().filter(|i| i.checked).map(|i| i.label.to_string()).collect())
    })
}

/// Choose `label` in the font list of the open document.
fn choose_font(s: &mut Scenario, label: &str) {
    use crate::shell::chrome::{MenuEntry, ToolItem};
    let label = label.to_string();
    let handler = s.with(move |e, _| {
        let Some(ToolItem::Select { menu, .. }) = e.toolbar_items().into_iter().find(|i| matches!(i, ToolItem::Select { id: "font", .. })) else { panic!("no font box") };
        menu.into_iter().find_map(|m| if let MenuEntry::Item(i) = m { (i.label.as_str() == label).then_some(i.handler) } else { None }).flatten().unwrap_or_else(|| panic!("no font {label} in the list"))
    });
    s.update(|e, window, cx| handler(e, window, cx));
}

/// #36 (3): the font lists are made from the fonts installed on this system, not a fixed table of
/// Office's names: ＭＳ ゴシック and メイリオ are not on a Mac and are not listed, Hiragino is (by its
/// name in the system's language), the hidden `.` fonts are not; the font a list sets is saved under
/// the listed name; a document's own font that is not installed shows in the box as it is.
fn fix36_font_lists_from_the_system() {
    let hiragino = |l: &[String]| l.iter().any(|n| n == "Hiragino Sans" || n == "ヒラギノ角ゴシック");
    for kind in [ibis_pptx::Kind::Workbook, ibis_pptx::Kind::Document, ibis_pptx::Kind::Presentation] {
        let mut s = if kind == ibis_pptx::Kind::Presentation { Scenario::open("test/editing/pptx-plan.pptx") } else { Scenario::new_document(kind) };
        // A presentation's font box is there while a shape is selected.
        s.update(|e, window, cx| {
            if kind == ibis_pptx::Kind::Presentation {
                select_a_text_shape(e, window, cx);
            }
        });
        let (_, list, _) = font_box(&mut s);
        if cfg!(target_os = "macos") {
            assert!(!list.is_empty(), "{kind:?}: no fonts listed");
            for gone in ["ＭＳ ゴシック", "ＭＳ Ｐゴシック", "ＭＳ 明朝", "メイリオ", "BIZ UDPゴシック"] {
                assert!(!list.iter().any(|n| n == gone), "{kind:?}: {gone} is listed on a Mac");
            }
            assert!(hiragino(&list), "{kind:?}: Hiragino is not listed");
        }
        assert!(!list.iter().any(|n| n.starts_with('.')), "{kind:?}: a hidden font is listed");
        assert!(list.windows(2).all(|w| w[0].is_ascii() != w[1].is_ascii() || w[0].to_lowercase() <= w[1].to_lowercase() || !w[0].is_ascii()), "{kind:?}: not sorted");
        let mut sorted = list.clone();
        sorted.dedup();
        assert_eq!(sorted.len(), list.len(), "{kind:?}: a family twice");
        eprintln!("fix36 {kind:?}: {} families, first {:?}, Hiragino {:?}", list.len(), &list[..list.len().min(4)], list.iter().filter(|n| n.contains("Hiragino") || n.contains("ヒラギノ")).collect::<Vec<_>>());
    }
    // A presentation: the font of the selected text is saved under the listed name.
    {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(select_a_text_shape);
    let (_, list, _) = font_box(&mut s);
    let chosen = list.iter().find(|n| *n == "Hiragino Sans" || *n == "ヒラギノ角ゴシック").cloned().unwrap_or_else(|| list[0].clone());
    choose_font(&mut s, &chosen);
    let (value, _, checked) = font_box(&mut s);
    assert_eq!(value, chosen);
    assert_eq!(checked, [chosen.clone()]);
    }
    // A workbook: choose Hiragino, the box shows the name it is saved under; a font the system lacks shows as it is.
    let mut s = Scenario::new_document(ibis_pptx::Kind::Workbook);
    s.update(|_, _, _| {});
    let (_, list, _) = font_box(&mut s);
    let chosen = list.iter().find(|n| *n == "Hiragino Sans" || *n == "ヒラギノ角ゴシック").cloned().unwrap_or_else(|| list[0].clone());
    choose_font(&mut s, &chosen);
    let (value, _, checked) = font_box(&mut s);
    assert_eq!(value, chosen);
    assert_eq!(checked, [chosen.clone()]);
    // The cell's style holds the listed name (what is saved in the workbook).
    assert_eq!(s.with(|e, _| e.current_style().font), Some(chosen.clone()));
    s.update(|e, _, cx| e.format_cells(ibis_doc_model::StyleChange::Font(Some("メイリオ".into())), cx));
    let (value, list, checked) = font_box(&mut s);
    assert_eq!(value, "メイリオ");
    assert!(checked.is_empty() && !list.iter().any(|n| n == "メイリオ"));
    s.screenshot("fix36-xlsx-font-not-installed");
}

/// The font list of a document and of a presentation starts with its dialog item ("Fonts…" for a
/// document, "Replace Fonts…" for a presentation), then a separator, then the fonts.
fn fix_font_dialog_item_leads_the_list() {
    use crate::shell::chrome::{MenuEntry, ToolItem};
    for (kind, id) in [(ibis_pptx::Kind::Document, "docx.font.custom"), (ibis_pptx::Kind::Presentation, "pptx.replace-fonts")] {
        let mut s = if kind == ibis_pptx::Kind::Presentation { Scenario::open("test/editing/pptx-plan.pptx") } else { Scenario::new_document(kind) };
        s.update(|e, window, cx| {
            if kind == ibis_pptx::Kind::Presentation {
                select_a_text_shape(e, window, cx);
            }
        });
        let (first, second, last) = s.with(|e, _| {
            let Some(ToolItem::Select { menu, .. }) = e.toolbar_items().into_iter().find(|i| matches!(i, ToolItem::Select { id: "font", .. })) else { panic!("no font box") };
            let name = |m: &MenuEntry| match m { MenuEntry::Item(i) => i.id.to_string(), MenuEntry::Separator => "-".into(), _ => "?".into() };
            (name(&menu[0]), name(&menu[1]), name(menu.last().unwrap()))
        });
        assert_eq!((first.as_str(), second.as_str()), (id, "-"), "{kind:?}");
        assert_ne!(last, id, "{kind:?}: the dialog item is also at the end");
    }
}

/// フォントの置換 on the document and the spreadsheet screens: the panel lists the fonts the file uses, and 置換 sets the other
/// font on every run (cell) drawn in the first one. The Edit menu has the item on both screens.
fn replace_fonts_document_and_workbook() {
    use ibis_doc_model::{CellEdit, CellInput, CellRange, Command, SheetCommand, StyleChange};
    // Document.
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.menu("edit.replace-fonts");
    let first = s.with(|e, _| e.replace_fonts.as_ref().and_then(|r| r.used_fonts.first().cloned())).expect("the report uses a font");
    s.update(|e, w, cx| {
        let r = e.replace_fonts.as_ref().unwrap();
        let (t, d) = (r.target.clone(), r.replacement.clone());
        let first = first.clone();
        t.update(cx, |f, cx| f.set_value(&first, w, cx));
        d.update(cx, |f, cx| f.set_value("Georgia", w, cx));
        e.replace_presentation_fonts(w, cx);
    });
    let fonts = s.with(|e, _| e.used_fonts());
    assert!(fonts.iter().any(|f| f == "Georgia") && !fonts.iter().any(|f| f.eq_ignore_ascii_case(&first)) || first == "Georgia", "{first} {fonts:?}");
    s.screenshot("replace-fonts-docx");
    drop(s);
    // Workbook.
    let mut s = Scenario::new_document(ibis_pptx::Kind::Workbook);
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        let range = CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 1 };
        e.run(vec![
            Command::Sheet(SheetCommand::SetCells { slide, edits: vec![CellEdit { row: 0, column: 0, input: CellInput::Value(ibis_doc_model::CellValue::Text("a".into())) }, CellEdit { row: 0, column: 1, input: CellInput::Value(ibis_doc_model::CellValue::Text("b".into())) }] }),
            Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![range], change: StyleChange::Font(Some("Courier New".into())) }),
        ], cx);
    });
    s.menu("edit.replace-fonts");
    assert!(s.with(|e, _| e.replace_fonts.as_ref().is_some_and(|r| r.used_fonts.iter().any(|f| f == "Courier New"))));
    s.update(|e, w, cx| {
        let r = e.replace_fonts.as_ref().unwrap();
        let (t, d) = (r.target.clone(), r.replacement.clone());
        t.update(cx, |f, cx| f.set_value("Courier New", w, cx));
        d.update(cx, |f, cx| f.set_value("Georgia", w, cx));
        e.replace_presentation_fonts(w, cx);
    });
    let fonts = s.with(|e, _| e.used_fonts());
    assert_eq!(fonts, vec!["Georgia".to_string()]);
    s.screenshot("replace-fonts-xlsx");
}
