// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > ブックのリンク…: the external workbooks the formulas read, each
//! with 値の更新 (the cached cells made again from its file, found by the
//! path the link names or chosen) and リンクの解除 (its formulas made values).
//! Until updated, the formulas calculate from the values the file cached.
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use crate::shell::icons::Material;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, SheetCommand};

pub(crate) struct ExternalLinksPanel {
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    pub(crate) fn open_external_links_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let state = self.wb_mut();
        state.custom_format = None;
        state.page_setup = false;
        state.names = None;
        state.validation = None;
        state.find = None;
        state.link = None;
        state.conditional_edit = None;
        state.sort_edit = None;
        state.close_settings();
        state.panels.external_links = Some(ExternalLinksPanel { message: String::new(), ui: Default::default() });
        self.sidebar_open = true;
        cx.notify();
    }

    fn external_links_message(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.external_links.as_mut() {
            p.message = text;
        }
        cx.notify();
    }

    /// The commands that link the workbooks a typed formula names that the file does not link yet
    /// (`=[Budget.xlsx]Sheet1!A1`): each found beside this workbook (its file read for its sheets and the cells
    /// the formula reads), added before the entry so that one undo takes both. A file not found is told in the
    /// status, and the formula stays as it was typed (`#REF!`).
    pub(crate) fn link_new_books_commands(&mut self, formula: &str) -> Vec<Command> {
        let books: Vec<String> = self.presentation().workbook.as_ref().map(|w| w.external_books.iter().filter(|b| !b.broken).map(|b| b.name.clone()).collect()).unwrap_or_default();
        let mut unknown: Vec<String> = Vec::new();
        for name in ibis_calc::formula::unknown_books(formula, &books) {
            if !unknown.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                unknown.push(name);
            }
        }
        let mut commands = Vec::new();
        for name in unknown {
            #[cfg(not(target_family = "wasm"))]
            let (path_str, data) = {
                let resolve_candidate = |candidate: std::path::PathBuf| -> Option<std::path::PathBuf> {
                    if candidate.is_absolute() {
                        return Some(candidate);
                    }
                    if let Some(base) = self.path.as_ref().and_then(|p| p.parent()) {
                        let mut cur: Option<&std::path::Path> = Some(base);
                        while let Some(dir) = cur {
                            let attempt = dir.join(&candidate);
                            if attempt.exists() {
                                return Some(attempt);
                            }
                            cur = dir.parent();
                        }
                    }
                    if candidate.exists() {
                        return Some(candidate);
                    }
                    self.path.as_ref().and_then(|base| base.parent()).map(|d| d.join(candidate))
                };
                let file_path = if let Some(p) = name.find('[') && let Some(e) = name[p + 1..].find(']') {
                    let dir_part = &name[..p];
                    let file_part = &name[p + 1..p + 1 + e];
                    let dir_part_clean = dir_part.replace('\\', "/");
                    if dir_part.is_empty() {
                        self.path.as_ref().and_then(|base| base.parent()).map(|d| d.join(file_part))
                    } else {
                        let full = std::path::PathBuf::from(&dir_part_clean).join(file_part);
                        resolve_candidate(full)
                    }
                } else {
                    let explicit_path = ibis_calc::formula::book_path_in_formula(formula, &name);
                    if let Some(ref p) = explicit_path {
                        let p_clean = p.replace('\\', "/");
                        let candidate = std::path::PathBuf::from(&p_clean).join(&name);
                        resolve_candidate(candidate)
                    } else if !name.contains(['/', '\\']) {
                        self.path.as_ref().and_then(|p| p.parent()).map(|d| d.join(&name))
                    } else {
                        let name_clean = name.replace('\\', "/");
                        let p = std::path::PathBuf::from(&name_clean);
                        resolve_candidate(p)
                    }
                };
                let found_data = file_path.as_ref().and_then(|p| std::fs::read(p).ok());
                let final_path = file_path.map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| name.clone());
                (final_path, found_data)
            };
            #[cfg(target_family = "wasm")]
            let (path_str, data): (String, Option<Vec<u8>>) = (name.clone(), None);
            let mut book = ibis_pptx::new_external_book(&name, data.as_deref(), formula);
            book.path = path_str;
            if book.sheets.is_empty() {
                self.status = Some(crate::shell::i18n::format("xlsx.external.file-not-found", &[("name", &name)]).into());
                continue;
            }
            commands.push(Command::Sheet(SheetCommand::AddExternalBook { book }));
        }
        commands
    }

    /// The links of the workbook still in use: (index, book).
    pub(crate) fn live_external_books(&self) -> Vec<(usize, ibis_doc_model::ExternalBook)> {
        self.presentation().workbook.as_ref().map(|w| w.external_books.iter().cloned().enumerate().filter(|(_, b)| !b.broken && !b.name.is_empty()).collect()).unwrap_or_default()
    }

    /// 値の更新 of link `index` from the file `data` (its bytes).
    pub(crate) fn refresh_external_book_from(&mut self, index: usize, data: &[u8], cx: &mut Context<Self>) {
        let Some(book) = self.presentation().workbook.as_ref().and_then(|w| w.external_books.get(index)).cloned() else { return };
        match ibis_pptx::refreshed_sheets(&book, data) {
            Ok(sheets) => {
                self.run_sheet(vec![Command::Sheet(SheetCommand::RefreshExternalBook { index, sheets })], cx);
                self.external_links_message(crate::shell::i18n::format("external-links.updated", &[("name", &book.name)]), cx);
            }
            Err(e) => self.external_links_message(e.to_string(), cx),
        }
    }

    /// The bytes of the file a link names: at the path the link gives, or else a file of that name in the folder of
    /// this workbook (a workbook moved with the files it links).
    #[cfg(not(target_family = "wasm"))]
    fn link_file(&self, book: &ibis_doc_model::ExternalBook) -> Option<Vec<u8>> {
        let dir = self.file_path().and_then(|p| p.parent().map(std::path::Path::to_path_buf));
        if let Some(data) = ibis_pptx::link_path(&book.path, dir.as_deref()).and_then(|p| std::fs::read(p).ok()) {
            return Some(data);
        }
        let name = ibis_pptx::link_path(&book.path, None).as_deref().and_then(|p| p.file_name().map(|n| n.to_owned())).or_else(|| (!book.name.is_empty()).then(|| book.name.clone().into()))?;
        std::fs::read(dir?.join(name)).ok()
    }

    /// 値の更新 from the file the link names.
    pub(crate) fn refresh_external_book(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(book) = self.presentation().workbook.as_ref().and_then(|w| w.external_books.get(index)).cloned() else { return };
        #[cfg(not(target_family = "wasm"))]
        let found = self.link_file(&book);
        #[cfg(target_family = "wasm")]
        let found: Option<Vec<u8>> = None;
        match found {
            Some(data) => self.refresh_external_book_from(index, &data, cx),
            None => self.external_links_message(crate::shell::i18n::format("external-links.not-found", &[("path", &book.path)]), cx),
        }
    }

    /// 値の更新 from a file chosen.
    pub(crate) fn refresh_external_book_picked(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false, extensions: Vec::new(),
                multiple: false,
                prompt: Some(crate::shell::i18n::text("_xlsx.update").to_string().into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                this.update(cx, |this, cx| this.refresh_external_book_from(index, &data, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Some((_, data))) = crate::web::pick_file().await else { return };
                this.update(cx, |this, cx| this.refresh_external_book_from(index, &data, cx)).ok();
            })
            .detach();
        }
    }

    /// リンク元の変更 of link `index` from bytes `data` of a chosen new workbook file.
    pub(crate) fn change_external_book_source(
        &mut self,
        index: usize,
        new_name: String,
        new_path: String,
        data: &[u8],
        cx: &mut Context<Self>,
    ) {
        let Ok(new_book) = ibis_pptx::read_external_book(&new_name, &new_path, data) else {
            self.external_links_message(crate::shell::i18n::format("xlsx.external.file-not-found", &[("name", &new_name)]).to_string(), cx);
            return;
        };
        self.run_sheet(
            vec![Command::Sheet(SheetCommand::ChangeExternalBookSource {
                index,
                new_name: new_name.clone(),
                new_path,
                sheets: new_book.sheets,
            })],
            cx,
        );
        self.external_links_message(crate::shell::i18n::format("xlsx.external.source-changed", &[("name", &new_name)]), cx);
    }

    /// リンク元の変更 from a file chosen.
    pub(crate) fn change_external_book_source_picked(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                extensions: vec!["xlsx".into(), "xlsm".into(), "xlsb".into()],
                multiple: false,
                prompt: Some(crate::shell::i18n::text("xlsx.external.change-source").to_string().into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Book.xlsx".to_string());
                let path_str = path.to_string_lossy().to_string();
                this.update(cx, |this, cx| this.change_external_book_source(index, name, path_str, &data, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Some((name, data))) = crate::web::pick_file().await else { return };
                this.update(cx, |this, cx| this.change_external_book_source(index, name.clone(), name, &data, cx)).ok();
            })
            .detach();
        }
    }

    /// 外部ブックのリンクを追加 from bytes `data` of a chosen workbook file.
    pub(crate) fn add_external_link(&mut self, name: String, path: String, data: &[u8], cx: &mut Context<Self>) {
        let Ok(book) = ibis_pptx::read_external_book(&name, &path, data) else {
            self.external_links_message(crate::shell::i18n::format("xlsx.external.file-not-found", &[("name", &name)]).to_string(), cx);
            return;
        };
        self.run_sheet(vec![Command::Sheet(SheetCommand::AddExternalBook { book })], cx);
        self.external_links_message(crate::shell::i18n::format("xlsx.external.link-added", &[("name", &name)]), cx);
    }

    /// 外部ブックのリンクを追加 from a file chosen.
    pub(crate) fn add_external_link_picked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                extensions: vec!["xlsx".into(), "xlsm".into(), "xlsb".into()],
                multiple: false,
                prompt: Some(crate::shell::i18n::text("xlsx.external.add-link").to_string().into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Book.xlsx".to_string());
                let path_str = path.to_string_lossy().to_string();
                this.update(cx, |this, cx| this.add_external_link(name, path_str, &data, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Some((name, data))) = crate::web::pick_file().await else { return };
                this.update(cx, |this, cx| this.add_external_link(name.clone(), name, &data, cx)).ok();
            })
            .detach();
        }
    }

    /// 起動時のリンクの確認: the bar shown on opening a workbook that links other workbooks.
    pub(crate) fn render_links_banner(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.sheet_state()?.links_notice {
            return None;
        }
        let count = self.live_external_books().len();
        if count == 0 {
            return None;
        }
        let text_color = rgb(0x7a4b00);
        Some(
            h_flex()
                .id("links-banner")
                .w_full()
                .py_1()
                .px_3()
                .bg(rgb(0xfff4ce))
                .border_b_1()
                .border_color(rgb(0xf0d58c))
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(Material("link")).size(px(16.)).text_color(text_color))
                        .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(text_color).child(crate::shell::i18n::format("xlsx.links-banner.text", &[("count", &count)]))),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_1()
                        .child(crate::shell::chrome::action_button("links-banner-update").small().ghost().label(crate::shell::i18n::text("xlsx.links-banner.update")).on_click(cx.listener(|this, _, _, cx| this.update_all_links(cx))))
                        .child(crate::shell::chrome::action_button("links-banner-edit").small().ghost().label(crate::shell::i18n::text("xlsx.links-banner.edit")).on_click(cx.listener(|this, _, window, cx| this.open_external_links_panel(window, cx))))
                        .child(crate::shell::chrome::action_button("links-banner-close").small().label(crate::shell::i18n::text("xlsx.links-banner.close")).on_click(cx.listener(|this, _, _, cx| this.dismiss_links_banner(cx)))),
                )
                .into_any_element(),
        )
    }

    pub(crate) fn dismiss_links_banner(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.sheet_state_mut() {
            state.links_notice = false;
        }
        cx.notify();
    }

    /// The bar's 値を更新: every link read again from the file it names (beside this workbook or at the path
    /// the link gives); those not found stay as cached, and the status says how many were updated.
    pub(crate) fn update_all_links(&mut self, cx: &mut Context<Self>) {
        let mut updated = 0usize;
        let mut missing: Vec<String> = Vec::new();
        for (index, book) in self.live_external_books() {
            #[cfg(not(target_family = "wasm"))]
            let data = self.link_file(&book);
            #[cfg(target_family = "wasm")]
            let data: Option<Vec<u8>> = None;
            match data.as_deref().map(|d| ibis_pptx::refreshed_sheets(&book, d)) {
                Some(Ok(sheets)) => {
                    self.run_sheet(vec![Command::Sheet(SheetCommand::RefreshExternalBook { index, sheets })], cx);
                    updated += 1;
                }
                _ => missing.push(book.name.clone()),
            }
        }
        if let Some(state) = self.sheet_state_mut() {
            state.links_notice = false;
        }
        let text = if missing.is_empty() {
            crate::shell::i18n::format("xlsx.links-banner.updated", &[("count", &updated)])
        } else {
            crate::shell::i18n::format("xlsx.links-banner.partly", &[("count", &updated), ("missing", &missing.join(", "))])
        };
        self.set_status(text.to_string(), cx);
    }

    /// リンクの解除.
    pub(crate) fn break_external_link(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.protection_refuses(crate::xlsx::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(name) = self.presentation().workbook.as_ref().and_then(|w| w.external_books.get(index)).map(|b| b.name.clone()) else { return };
        self.run_sheet(vec![Command::Sheet(SheetCommand::BreakExternalLink { index })], cx);
        self.external_links_message(crate::shell::i18n::format("external-links.broken", &[("name", &name)]), cx);
    }

    pub(crate) fn external_links_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let books = self.live_external_books();
        let p = self.sheet_state()?.panels.external_links.as_ref()?;
        let mark = |id: String, child: AnyElement| {
            let ui = p.ui.clone();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: String, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        if books.is_empty() {
            body = body.child(crate::shell::i18n::text("_xlsx.this-workbook-has-no-links"));
        }
        for (index, book) in books {
            let sheets = book.sheets.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ");
            body = body
                .child(div().font_weight(FontWeight::BOLD).child(SharedString::from(book.name.clone())))
                .child(div().text_sm().child(SharedString::from(ibis_pptx::link_display(&book.path))))
                .child(div().text_sm().child(crate::shell::i18n::format("external-links.sheets", &[("sheets", &sheets)])))
                .child(
                    h_flex()
                        .gap_1()
                        .flex_wrap()
                        .child(mark(format!("update-{index}"), crate::shell::chrome::action_button(SharedString::from(format!("external-update-{index}"))).small().label(crate::shell::i18n::text("_xlsx.update-values")).on_click(cx.listener(move |this, _, _, cx| this.refresh_external_book(index, cx))).into_any_element()))
                        .child(mark(format!("pick-{index}"), crate::shell::chrome::action_button(SharedString::from(format!("external-pick-{index}"))).small().label(crate::shell::i18n::text("_xlsx.update-from-file")).on_click(cx.listener(move |this, _, window, cx| this.refresh_external_book_picked(index, window, cx))).into_any_element()))
                        .child(mark(format!("change-{index}"), crate::shell::chrome::action_button(SharedString::from(format!("external-change-{index}"))).small().label(crate::shell::i18n::text("xlsx.external.change-source")).on_click(cx.listener(move |this, _, window, cx| this.change_external_book_source_picked(index, window, cx))).into_any_element()))
                        .child(mark(format!("break-{index}"), crate::shell::chrome::destructive_button(SharedString::from(format!("external-break-{index}")),cx).small().label(crate::shell::i18n::text("_xlsx.break-link")).on_click(cx.listener(move |this, _, _, cx| this.break_external_link(index, cx))).into_any_element())),
                );
        }
        body = body.child(
            mark(
                "add-link".into(),
                crate::shell::chrome::action_button("external-add-link")
                    .small()
                    .label(crate::shell::i18n::text("xlsx.external.add-link"))
                    .on_click(cx.listener(|this, _, window, cx| this.add_external_link_picked(window, cx)))
                    .into_any_element(),
            ),
        );
        body = body.children(crate::shell::chrome::panel_message(&p.message));
        let panel_terminal_actions = crate::shell::chrome::action_bar().action(mark_action("close".into(), crate::shell::chrome::ActionDefinition::new("external-links-close", "", crate::shell::chrome::ActionButtonKind::Normal)
                .label(crate::shell::i18n::text("_shell.close"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.wb_mut().panels.external_links = None;
                    cx.notify();
                }))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("_xlsx.workbook-links"), body.into_any_element(), panel_terminal_actions))
    }
}
