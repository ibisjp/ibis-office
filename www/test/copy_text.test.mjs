// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// B-6: copyText (crates/ibis-office-shell/js/web_scripts.js) never rejects: a browser that refuses the clipboard gives `false`, which the editor
// tells in the status bar (an unhandled rejection would only reach the console).
import assert from 'node:assert/strict';
import test from 'node:test';

import { copyText } from '../../crates/ibis-office-shell/js/web_scripts.js';

function setClipboard(clipboard) {
  Object.defineProperty(globalThis, 'navigator', { value: { clipboard }, configurable: true });
}

test('copyText: true when written, false when the browser refuses or has no clipboard', async () => {
  const written = [];
  setClipboard({ writeText: async (text) => void written.push(text) });
  assert.equal(await copyText('https://x.example/a'), true);
  assert.deepEqual(written, ['https://x.example/a']);
  setClipboard({
    writeText: async () => {
      const error = new Error('Write permission denied.');
      error.name = 'NotAllowedError';
      throw error;
    },
  });
  assert.equal(await copyText('x'), false);
  setClipboard(undefined);
  assert.equal(await copyText('x'), false);
});
