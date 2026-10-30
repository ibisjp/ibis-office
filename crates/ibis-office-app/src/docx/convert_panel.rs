// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Document import/export backend. Format choice lives in File > Export;
//! filenames are selected by the operating system, without a conversion dialog.
use std::path::Path;
use gpui_kit::*;
use ibis_pptx::docx_convert::{DocxConvertFormat, export_docx_format, import_docx_format};
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

impl SlideEditor {
    /// File > Export > a supported text-document format.
    pub(crate) fn export_docx_format_dialog(&mut self, format: DocxConvertFormat, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            let name = format!("{stem}.{}", format.extension());
            let directory = self.file_path().and_then(|p| p.parent().map(Path::to_path_buf))
                .or_else(|| std::env::var_os("HOME").map(Into::into)).unwrap_or_else(|| ".".into());
            let picked = cx.prompt_for_new_path(&directory, Some(&name));
            cx.spawn_in(window, async move |editor, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                let _ = editor.update_in(cx, |editor, _, cx| {
                    if let Err(error) = editor.export_docx_to_file(format, &path, cx) { editor.set_status(error, cx); }
                });
            }).detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = format;
            self.report(Err(text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
        }
    }

    /// Export the current DOCX document to the given format and file path.
    pub(crate) fn export_docx_to_file(
        &mut self,
        format: DocxConvertFormat,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> Result<usize, String> {
        let pres = self.presentation().clone();
        let bytes = export_docx_format(&pres, format).map_err(|e| e.to_string())?;
        let len = bytes.len();
        std::fs::write(path, &bytes).map_err(|e| format!("{}: {}", path.display(), e))?;
        self.set_status(
            format!("{}: {}", text("docx.convert.exported"), path.display()),
            cx,
        );
        Ok(len)
    }

    /// Import an external document (`.doc`, `.rtf`, `.txt`, `.html`, `.odt`, `.pdf`) into the editor as a DOCX document.
    pub(crate) fn import_docx_from_file(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<DocxConvertFormat, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        let format = DocxConvertFormat::from_path(path)
            .or_else(|| DocxConvertFormat::sniff(&bytes))
            .unwrap_or(DocxConvertFormat::Txt);
        let opened = import_docx_format(&bytes, format).map_err(|e| e.to_string())?;
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Imported".to_string());
        let doc_name: SharedString = format!("{stem}.docx").into();
        self.load(opened.presentation, doc_name, window, cx);
        self.source = Some(std::sync::Arc::new(opened.source));
        self.set_file_path(Some(path.with_extension("docx")));
        self.set_status(
            format!("{}: {}", text("docx.convert.imported"), path.display()),
            cx,
        );
        Ok(format)
    }

}
