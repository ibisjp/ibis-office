// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// The web build's browser-side helpers of ibis-office-shell (web.rs). Functions of this file, imported by wasm-bindgen, not source text built into a
// function at run time (`new Function` needs `unsafe-eval`, which the Content-Security-Policy of a host page does not give).

/** Opens this page in a new tab with the query `q` (`""` or `"?new=docx"`). */
export const openInNewTab = (q) => {
  window.open(location.origin + location.pathname + q, '_blank');
};

/** The `?new=` of this page's address (`docx`, `xlsx`, `pptx`), or `null`. */
export const newKindOfAddress = () => new URLSearchParams(location.search).get('new');

/** Copies `text` to the clipboard: `true` when it was written, `false` when the browser refused (no permission, not a secure context, no focus). */
export const copyText = async (text) => {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
};
