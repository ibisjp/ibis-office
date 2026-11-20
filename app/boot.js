"use strict";
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// Runs before main.js: the page's language, for the words shown while the editor loads. It is a file of its own, not an inline
// script, so that the page runs under a Content-Security-Policy of `script-src 'self'`.
(() => {
    // The same language as the playground page's menu, else the browser's. A page started for a host (`?host=1`) shares its storage
    // with the host and does not read it (the host's `locale` is applied by main.js).
    let lang = null;
    if (!new URLSearchParams(location.search).has('host')) {
        try {
            lang = localStorage.getItem('ibisoffice-lang'); // host-lint-allow: playground only
        }
        catch {
            // storage blocked
        }
    }
    if ((lang ?? navigator.language).toLowerCase().startsWith('ja')) {
        document.documentElement.lang = 'ja';
        const text = document.getElementById('status-text');
        if (text) {
            text.textContent = 'ibisOffice を読み込んでいます…';
        }
    }
})();
