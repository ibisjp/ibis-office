// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

use gpui_kit::*;

use crate::{editor::SlideEditor, i18n::text};
use ibis_formats::lawtext::{LawDoc, open_comparison_document, parse_law_input};

fn law_file(path: &std::path::Path) -> bool {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    name.ends_with(".xml") || name.ends_with(".lawtext") || name.ends_with(".law.txt")
}

fn law_from_file(path: &std::path::Path) -> Result<LawDoc, String> {
    let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_law_input(&source).map_err(|e| e.to_string())
}

/// The statute held by the window's current content (via Markdown). Placed here, not in the fork's `ai_assistant.rs`, which is not part of this tree.
fn presentation_law(presentation: &ibis_doc_model::Presentation) -> Result<LawDoc, String> {
    let markdown = ibis_formats::markdown::export_presentation_to_markdown(presentation).map_err(|e| e.to_string())?;
    let law = ibis_formats::lawtext::parse_markdown_law(&markdown).map_err(|e| e.to_string())?;
    if law.main_provision.is_empty() && law.suppl_provisions.is_empty() && law.appdx_tables.is_empty() {
        return Err(text("compare.no-law-body").to_string());
    }
    Ok(law)
}

fn label(name: &str, law: &LawDoc) -> String {
    [name, law.law_title.as_str(), law.law_num.as_str()]
        .into_iter()
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

impl SlideEditor {
    pub(crate) fn has_law_source(&self) -> bool {
        if self.kind() != ibis_formats::Kind::Document || self.loading {
            return false;
        }
        self.file_path().as_deref().is_some_and(|path| {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase();
            name.ends_with(".lawtext") || name.ends_with(".law.txt")
        }) || self.presentation().flow.as_ref().is_some_and(|flow| {
            flow.body.iter().take(8).any(|block| match block {
                ibis_doc_model::flow::Block::Paragraph(p) => {
                    p.text().trim_start().starts_with("法令番号:") // i18n-ok: the marker of a statute in law Markdown, data not UI text
                }
                _ => false,
            })
        })
    }

    pub(crate) fn compare_documents(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.has_document || self.loading || self.kind() != ibis_formats::Kind::Document {
            return;
        }
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            extensions: ["xml", "lawtext", "txt", "md"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            multiple: false,
            prompt: Some(text("compare.pick-baseline")),
        });

        cx.spawn_in(window, async move |this, cx| {
            let paths = match picked.await {
                Ok(Ok(Some(paths))) => paths,
                Ok(Err(error)) => {
                    let _ = this.update(cx, |editor, cx| {
                        editor.show_alert(text("compare.failed"), error.to_string().into(), cx);
                    });
                    return;
                }
                _ => return,
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update_in(cx, |editor, window, cx| {
                editor.compare_law_with_file(path, window, cx);
            });
        })
        .detach();
    }

    /// The selected file contains a recognized statute input.
    pub fn is_law_comparison_file(path: &std::path::Path) -> bool {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        if name.ends_with(".lawtext") || name.ends_with(".law.txt") {
            return true;
        }
        if ![".xml", ".md", ".txt"]
            .iter()
            .any(|ext| name.ends_with(ext))
        {
            return false;
        }
        use std::io::Read;
        let Ok(file) = std::fs::File::open(path) else {
            return false;
        };
        let mut data = Vec::new();
        if file.take(8192).read_to_end(&mut data).is_err() {
            return false;
        }
        // A bounded probe can end in the middle of a UTF-8 character.
        if let Err(error) = std::str::from_utf8(&data) {
            if error.error_len().is_some() {
                return false;
            }
            data.truncate(error.valid_up_to());
        }
        if name.ends_with(".xml") {
            return ibis_formats::lawtext::is_law_xml(&data);
        }
        if name.ends_with(".txt") {
            return ibis_formats::lawtext::is_lawtext(&data);
        }
        std::str::from_utf8(&data).is_ok_and(|head| {
            let head = head.trim_start_matches('\u{feff}').trim_start();
            head.starts_with("---") && head.contains("type: lawtext")
        })
    }

    /// Open a statute comparison from the normal document-comparison command.
    pub fn compare_law_with_file(
        &mut self,
        path: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.has_document || self.loading || self.kind() != ibis_formats::Kind::Document {
            return;
        }
        self.commit_edit(window, cx);
        let active = self.presentation().clone();
        let active_name = self
            .file_name
            .clone()
            .unwrap_or_else(|| text("file.new.document"));
        let original = self
            .file_path()
            .filter(|path| !self.is_dirty() && law_file(path));
        let known = self.watch.known.clone();
        cx.spawn_in(window, async move |this, cx| {
            let baseline_name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let inputs = cx
                .background_executor()
                .spawn(async move {
                    let baseline = law_from_file(&path)?;
                    // Prefer the original markup for an unedited file. For an edited
                    // document, compare the snapshot of the window's current content.
                    let current = original.as_deref().and_then(|path| {
                        if known.is_none() || crate::file_watch::DiskStamp::stat(path) != known {
                            return None;
                        }
                        let law = law_from_file(path).ok()?;
                        (crate::file_watch::DiskStamp::stat(path) == known).then_some(law)
                    });
                    let working = match current {
                        Some(law) => law,
                        None => presentation_law(&active)?,
                    };
                    if !baseline.law_num.is_empty()
                        && !working.law_num.is_empty()
                        && baseline.law_num != working.law_num
                    {
                        return Err(format!(
                            "{}\n{} / {}",
                            text("compare.different-laws"),
                            baseline.law_num,
                            working.law_num
                        ));
                    }
                    Ok::<_, String>((baseline, working))
                })
                .await;
            let (mut old, mut new) = match inputs {
                Ok(laws) => laws,
                Err(error) => {
                    let _ = this.update(cx, |editor, cx| {
                        editor.show_alert(text("compare.failed"), error.into(), cx)
                    });
                    return;
                }
            };
            let mut old_name = baseline_name.as_str();
            let mut new_name = active_name.as_ref();
            loop {
                let detail = format!(
                    "{}\n{}\n\n{}\n{}",
                    text("compare.current"),
                    label(old_name, &old),
                    text("compare.amended"),
                    label(new_name, &new),
                );
                let buttons = [
                    PromptButton::ok(text("compare.open-result")),
                    PromptButton::ok(text("compare.swap")),
                    PromptButton::cancel(text("_shell.cancel")),
                ];
                let Ok(receiver) = this.update_in(cx, |_, window, cx| {
                    window.prompt(
                        PromptLevel::Info,
                        &text("compare.law-preview"),
                        Some(&detail),
                        &buttons,
                        cx,
                    )
                }) else {
                    return;
                };
                match receiver.await {
                    Ok(0) => break,
                    Ok(1) => {
                        std::mem::swap(&mut old, &mut new);
                        std::mem::swap(&mut old_name, &mut new_name);
                    }
                    _ => return,
                }
            }
            let title = if new.law_title.is_empty() {
                &old.law_title
            } else {
                &new.law_title
            };
            let name = format!("{title} {}.docx", text("compare.law-result"));
            let opened = cx
                .background_executor()
                .spawn(async move { open_comparison_document(&old, &new) })
                .await;
            let _ = this.update(cx, |editor, cx| match opened {
                Ok(document) => crate::app::open_window_with(cx, move |editor, window, cx| {
                    editor.load(document.presentation, name.into(), window, cx);
                    editor.source = Some(std::sync::Arc::new(document.source));
                }),
                Err(error) => {
                    editor.show_alert(text("compare.failed"), error.to_string().into(), cx)
                }
            });
        })
        .detach();
    }
}
