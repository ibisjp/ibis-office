// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The browser version's PDF for 書き出し > PDF and 印刷 (K0037): made in the page by the app's export (`app_hooks::web_pdf`), then
//! handed to the host's storage or the download (an export), or shown in a tab (a print).

use gpui_kit::*;

use crate::editor::SlideEditor;
use crate::format::with_format;
use crate::i18n::{format, text};

/// What the app's PDF is: made in the page, over some time (the fonts of the PC are asked for).
pub type PdfFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Result<WebPdf, String>>>>;

/// What the app made.
pub struct WebPdf {
    pub bytes: Vec<u8>,
    /// Characters left out for want of their font's data.
    pub missing_glyphs: usize,
    /// The print settings asked for pictures of the pages (colour, frame, handouts, notes, outline), which the browser cannot make: the
    /// pages came out as they are.
    pub pictures_skipped: bool,
}

/// What the PDF is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Purpose {
    Export,
    Print,
}

/// The empty tab a print without a host opens at the click (the browser's own; with a host, the host opens its own: `printBegin`).
#[cfg(target_family = "wasm")]
type Tab = crate::web::PrintTab;
#[cfg(not(target_family = "wasm"))]
type Tab = ();

fn open_tab() -> Option<Tab> {
    #[cfg(target_family = "wasm")]
    return Some(crate::web::open_print_tab());
    #[cfg(not(target_family = "wasm"))]
    None
}

fn close_tab(tab: Tab) {
    #[cfg(target_family = "wasm")]
    crate::web::close_print_tab(tab);
    #[cfg(not(target_family = "wasm"))]
    let _ = tab;
}

#[cfg(target_family = "wasm")]
impl SlideEditor {
    /// ファイル > 書き出し > PDF in the browser.
    pub(crate) fn web_export_pdf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.web_pdf(Purpose::Export, window, cx);
    }

    /// ファイル > 印刷 in the browser: the PDF in a tab, to print with the browser's viewer.
    pub(crate) fn web_print_pdf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.web_pdf(Purpose::Print, window, cx);
    }

    fn web_pdf(&mut self, purpose: Purpose, window: &mut Window, cx: &mut Context<Self>) {
        let printing = purpose == Purpose::Print;
        let failed = move |this: &mut Self, error: &str, cx: &mut Context<Self>| {
            this.report(Err(format(if printing { "file.print-failed" } else { "file.pdf-failed" }, &[("error", &error)])), cx);
        };
        // `features.export` of the host decides before the work is done.
        if !printing && !crate::host::export_allowed("pdf") {
            self.set_status(text("shell.host.export-not-allowed").to_string(), cx);
            return;
        }
        let job = match with_format!(self, pdf_job(self, printing)) {
            Ok(job) => job,
            Err(error) => return failed(self, &error, cx),
        };
        let extra = with_format!(self, export_args(self));
        let (stem, ext) = self.name_parts();
        let file_name = format!("{stem}.{ext}");
        // The fonts of the PC are asked for at the click too (the browser's permission prompt wants it).
        crate::web::local_fonts_start();
        let made: PdfFuture = crate::app_hooks::web_pdf(job, file_name, extra);
        self.web_pdf_deliver(purpose, stem, made, window, cx);
    }
}

impl SlideEditor {
    /// The PDF being made (`made`) is handed over when it is ready: a print to the host's `printPdf` when a host gave one (the host
    /// opens its empty tab at `printBegin`, called now, while the click counts), else to a tab of the browser opened now; an export to the
    /// host's storage through its file window, or the download. One path for the browser with a host and without.
    pub fn web_pdf_deliver(&mut self, purpose: Purpose, stem: String, made: PdfFuture, window: &mut Window, cx: &mut Context<Self>) {
        let printing = purpose == Purpose::Print;
        let failed = move |this: &mut Self, error: &str, cx: &mut Context<Self>| {
            this.report(Err(format(if printing { "file.print-failed" } else { "file.pdf-failed" }, &[("error", &error)])), cx);
        };
        let host_print = printing.then(|| crate::host::bridge_with(|c| c.print_pdf)).flatten();
        // The tab is opened now, while the click counts: the popup blocker stops one opened after the PDF is made.
        let tab = if printing && host_print.is_none() { open_tab() } else { None };
        self.report(Ok(text("shell.web-pdf.making").to_string()), cx);
        cx.spawn_in(window, async move |this, cx| {
            if let Some(bridge) = &host_print {
                bridge.print_begin().await;
            }
            // The status bar is drawn before the page's one thread is taken by the export (the browser's: a desktop has threads, and a test's
            // settle does not wait for a timer).
            #[cfg(target_family = "wasm")]
            cx.background_executor().timer(std::time::Duration::from_millis(50)).await;
            let made = made.await;
            match made {
                Ok(made) => {
                    this.update_in(cx, |this, window, cx| this.web_pdf_made(purpose, stem, made, tab, host_print, window, cx)).ok();
                }
                Err(error) => {
                    if let Some(bridge) = &host_print {
                        bridge.print_cancel().await;
                    }
                    this.update(cx, |this, cx| {
                        if let Some(tab) = tab {
                            close_tab(tab);
                        }
                        failed(this, &error, cx)
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn web_pdf_made(&mut self, purpose: Purpose, stem: String, made: WebPdf, tab: Option<Tab>, host_print: Option<std::rc::Rc<dyn crate::host::HostBridge>>, window: &mut Window, cx: &mut Context<Self>) {
        let note = if made.pictures_skipped {
            Some(text("shell.web-pdf.pictures-skipped").to_string())
        } else if made.missing_glyphs > 0 {
            Some(format("shell.web-pdf.missing-glyphs", &[("count", &made.missing_glyphs)]))
        } else {
            None
        };
        let name = format!("{stem}.pdf");
        if purpose == Purpose::Print {
            let opened = note.unwrap_or_else(|| text("shell.web-pdf.print-opened").to_string());
            if let Some(bridge) = host_print {
                cx.spawn_in(window, async move |this, cx| {
                    let answer = bridge.print_pdf(name, made.bytes).await;
                    this.update(cx, |this, cx| match answer {
                        Ok(()) => this.report(Ok(opened), cx),
                        Err(failure) => {
                            let message = failure.message.filter(|m| !m.is_empty()).unwrap_or_else(|| text("shell.host.error-other").to_string());
                            this.report(Err(format("file.print-failed", &[("error", &message)])), cx)
                        }
                    })
                    .ok();
                })
                .detach();
                return;
            }
            #[cfg(target_family = "wasm")]
            if let Some(tab) = tab {
                crate::web::show_pdf_in_tab(tab, &made.bytes);
                self.report(Ok(opened), cx);
                return;
            }
            #[cfg(not(target_family = "wasm"))]
            let _ = (tab, opened);
            return;
        }
        // The host's file window (the PDF goes to its storage; `features.export` decides) or the download; with the window the PDF is not
        // yet written.
        if self.browser_export_noted(name, "application/pdf", made.bytes, note.clone(), window, cx) {
            return;
        }
        self.report(Ok(note.unwrap_or_else(|| text("shell.web-pdf.exported").to_string())), cx);
    }
}
