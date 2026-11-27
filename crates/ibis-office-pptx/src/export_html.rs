// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ファイル > 書き出し > Web ページ（.html）: the shown slides as one HTML page (`ibis_formats::slides_html`, GitHub issue #208): a save
//! dialog on the desktop, a download of the browser in the Web version.

use gpui_kit::*;

use crate::shell::editor::SlideEditor;

/// The words the page writes itself, in the UI's language.
fn labels() -> ibis_formats::slides_html::Labels {
    use crate::shell::i18n::text;
    ibis_formats::slides_html::Labels {
        chart: text("pptx.html-label-chart").to_string(),
        diagram: text("pptx.html-label-diagram").to_string(),
        picture: text("pptx.html-label-picture").to_string(),
        object: text("pptx.html-label-object").to_string(),
        slide: text("pptx.html-label-slide").to_string(),
    }
}

/// The page for a presentation (the title is the file's name), or what to tell the user.
pub(crate) fn page_for(presentation: &ibis_doc_model::Presentation, title: &str, english: bool) -> Result<String, String> {
    use ibis_formats::slides_html::{SlidesHtmlError, slides_to_html};
    slides_to_html(presentation, title, if english { "en" } else { "ja" }, &labels()).map_err(|e| match e {
        SlidesHtmlError::NoSlides => crate::shell::i18n::text("pptx.html-no-slides").to_string(),
        SlidesHtmlError::TooLarge { .. } => crate::shell::i18n::text("pptx.html-too-large").to_string(),
    })
}

#[ibis_office_macros::editor_ext(ExportHtml)]
impl SlideEditor {
    /// ファイル > 書き出し > Web ページ（.html）.
    pub(crate) fn export_html(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        let (stem, _) = self.name_parts();
        let title = if stem.is_empty() { "Presentation".to_string() } else { stem.clone() };
        let english = crate::shell::i18n::language() == crate::shell::i18n::Language::English;
        let page = page_for(self.presentation(), &title, english);
        // The browser makes the page itself, with the pictures (below): here it only asks whether there is a page.
        #[cfg(target_family = "wasm")]
        if let Err(message) = page {
            return self.report(Err(message), cx);
        }
        #[cfg(not(target_family = "wasm"))]
        let html = match page {
            Ok(html) => html,
            Err(message) => return self.report(Err(message), cx),
        };
        let name = format!("{title}.html");
        #[cfg(not(target_family = "wasm"))]
        {
            let directory = self.path.as_deref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf).or_else(|| std::env::var_os("HOME").map(Into::into)).unwrap_or_else(|| ".".into());
            let picked = cx.prompt_for_new_path(&directory, Some(&name));
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                // The page with its charts and SmartArt as pictures (`--export html`, #208); without the program, the page as it is made here (their places framed).
                let job = this.update(cx, |this, _| this.write_html_with_pictures(path.clone(), None)).ok().and_then(Result::ok);
                let result = match job {
                    Some(job) => cx.background_executor().spawn(async move { job() }).await.map(|written| crate::shell::i18n::format("pptx.html-exported", &[("path", &written.display())]).to_string()),
                    None => std::fs::write(&path, html).map(|()| crate::shell::i18n::format("pptx.html-exported", &[("path", &path.display())]).to_string()).map_err(|e| e.to_string()),
                }
                .map_err(|e| crate::shell::i18n::format("pptx.html-export-failed", &[("error", &e)]).to_string());
                this.update(cx, |this, cx| this.report(result, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            // The charts and SmartArt come as pictures drawn in this window into textures (`shell::html_raster`, #208).
            let presentation = self.presentation().clone();
            let (title, lang) = (title.clone(), if english { "en" } else { "ja" });
            self.html_with_pictures(
                move |raster| ibis_formats::slides_html::slides_to_html_with(&presentation, &title, lang, &labels(), raster).unwrap_or_default(),
                move |this, html, window, cx| {
                    // A host that keeps the files takes the export in its file window; else it is downloaded.
                    if !this.browser_export(name.clone(), "text/html;charset=utf-8", html.into_bytes(), window, cx) {
                        this.report(Ok(crate::shell::i18n::format("pptx.html-exported", &[("path", &name)]).to_string()), cx);
                    }
                },
                window,
                cx,
            );
        }
    }
}
