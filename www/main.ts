// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// Boots the wasm editor. `pkg/` is produced by scripts/build-web.sh.
import init, { run, set_embed_options } from './pkg/ibis_office_app.js';

/** A file chosen by the user or loaded at startup. */
interface PickedFile {
  name: string;
  bytes: Uint8Array;
}

/**
 * How a page that embeds the editor (a cloud service's preview) sets it up.
 * Read before the editor starts, from `window.ibisOfficeOptions` (set by a
 * script before main.js) over the URL's parameters. See
 * publish/doc/web-embedding.md.
 */
interface EmbedOptions {
  /** Show 閉じる at the top right (`?close=1`). Default: not shown. */
  close?: boolean;
  /** Show ダウンロード, left of it (`?download=1`). Default: not shown. */
  download?: boolean;
  /** Open in read-only viewing mode (`?view=1`). Default: false (editable). */
  view?: boolean;
  /** Show the menu bar (`?menuBar=1`, `?menuBar=0` hides it). Default: true (including when `view=1`). */
  menuBar?: boolean;
  /** Show the toolbar (`?toolBar=1`, `?toolBar=0` hides it). Default: true (including when `view=1`). */
  toolBar?: boolean;
  /**
   * The origin the parent window is sent messages for (`?origin=https://app.example.com`),
   * when the editor is in an iframe. Without it the referrer's origin is used;
   * without that nothing is posted to the parent. `'*'` (any origin) only when
   * given explicitly.
   */
  targetOrigin?: string;
  /** Where the 'ibisoffice:close' and 'ibisoffice:download' events go (JavaScript only). Default: window. */
  eventTarget?: EventTarget;
}

declare global {
  interface Window {
    /** Called from Rust for ファイル → 開く. */
    ibisOfficePickFile: () => Promise<PickedFile | null>;
    /** Called from Rust for the CMYK profile setting (ICC file picker, kept in IndexedDB). */
    ibisOfficePickIcc: () => Promise<PickedFile | null>;
    ibisOfficeIccSave: (name: string, bytes: Uint8Array) => void;
    ibisOfficeIccForget: () => void;
    ibisOfficeIccLoad: () => Promise<PickedFile | null>;
    /** Called from Rust for セットアップ (the fonts that can be downloaded; kept in IndexedDB in chunks). */
    ibisOfficeFontFetchText: (url: string) => Promise<string>;
    ibisOfficeFontsInstalled: () => Promise<{ file: string; size: number }[]>;
    ibisOfficeFontDownload: (file: string, url: string, size: number) => Promise<number>;
    ibisOfficeFontsLoad: () => Promise<Uint8Array[]>;
    /** Called from Rust when the close or the download button is pressed. */
    ibisOfficeEmbedAction: (kind: string, name: string, bytes?: Uint8Array) => void;
    /** Set by the embedding page before main.js. */
    ibisOfficeOptions?: EmbedOptions;
  }
}

/** `?name=1` / `?name=true` / `?name` is on; `0`, `false` is off; absent is `undefined`. */
function flag(params: URLSearchParams, name: string): boolean | undefined {
  if (!params.has(name)) {
    return undefined;
  }
  const value = (params.get(name) ?? '').toLowerCase();
  return !(value === '0' || value === 'false' || value === 'no' || value === 'off');
}

/** The embedding options: JavaScript's over the URL's, and everything off by default. */
function embedOptions(): Required<Omit<EmbedOptions, 'eventTarget'>> & Pick<EmbedOptions, 'eventTarget'> {
  const params = new URLSearchParams(location.search);
  const js = window.ibisOfficeOptions ?? {};
  const view = js.view ?? flag(params, 'view') ?? false;
  return {
    close: js.close ?? flag(params, 'close') ?? false,
    download: js.download ?? flag(params, 'download') ?? false,
    view,
    menuBar: js.menuBar ?? flag(params, 'menuBar') ?? flag(params, 'menubar') ?? true,
    toolBar: js.toolBar ?? flag(params, 'toolBar') ?? flag(params, 'toolbar') ?? true,
    targetOrigin: js.targetOrigin ?? params.get('origin') ?? '',
    eventTarget: js.eventTarget,
  };
}

/** The origin messages to the parent window go to, or `null` for nowhere. */
function parentOrigin(option: string): string | null {
  if (option) {
    return option;
  }
  try {
    return document.referrer ? new URL(document.referrer).origin : null;
  } catch {
    return null;
  }
}

/**
 * Called from Rust when the close or the download button is pressed: the
 * page that embeds the editor gets a CustomEvent ('ibisoffice:close', or
 * 'ibisoffice:download' with `{ name, bytes, blob }`) on `window` (or the
 * `eventTarget` given), and, in an iframe, a message to the parent window
 * (`{ source: 'ibisoffice', type: 'close' | 'download', name, bytes }`).
 */
function embedAction(kind: string, name: string, bytes?: Uint8Array): void {
  const options = embedOptions();
  const detail = kind === 'download' && bytes ? { name, bytes, blob: new Blob([bytes as BlobPart]) } : {};
  (options.eventTarget ?? window).dispatchEvent(new CustomEvent(`ibisoffice:${kind}`, { detail }));
  if (window.parent !== window) {
    const origin = parentOrigin(options.targetOrigin);
    if (origin === null) {
      console.warn('ibisOffice: no target origin for the parent window (set ?origin=… or ibisOfficeOptions.targetOrigin); nothing was posted to it.');
      return;
    }
    // The parent gets its own copy of the bytes (transferred), so the event's listeners keep theirs.
    if (kind === 'download' && bytes) {
      const copy = bytes.slice().buffer;
      window.parent.postMessage({ source: 'ibisoffice', type: kind, name, bytes: copy }, origin, [copy]);
    } else {
      window.parent.postMessage({ source: 'ibisoffice', type: kind }, origin);
    }
  }
}

/** Show the browser's file picker. Resolves to `null` when cancelled. */
function pickFile(accept?: string): Promise<PickedFile | null> {
  return new Promise((resolve, reject) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = accept ?? '.pptx,.xlsx,.docx,.ppt,.xls,.doc,application/vnd.openxmlformats-officedocument.presentationml.presentation,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,application/vnd.openxmlformats-officedocument.wordprocessingml.document';
    input.addEventListener('change', () => {
      const file = input.files?.[0];
      if (!file) {
        resolve(null);
        return;
      }
      file.arrayBuffer().then((buffer) => resolve({ name: file.name, bytes: new Uint8Array(buffer) }), reject);
    });
    input.addEventListener('cancel', () => resolve(null));
    input.click();
  });
}

/** Show the browser's file picker for a CMYK ICC profile (the CMYK mode's setting). Resolves to `null` when cancelled. */
function pickIcc(): Promise<PickedFile | null> {
  return pickFile('.icc,.icm');
}

/**
 * The CMYK profile the user chose is kept in the browser's own IndexedDB (never a server), as a setting: it stays after the
 * page is closed. One record, 'cmyk-profile'.
 */
function settingsDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open('ibisoffice-settings', 1);
    open.onupgradeneeded = () => open.result.createObjectStore('files');
    open.onerror = () => reject(open.error);
    open.onsuccess = () => resolve(open.result);
  });
}

function iccSave(name: string, bytes: Uint8Array): void {
  // Copy: the bytes Rust hands over are a view into its memory.
  const record = { name, bytes: bytes.slice() };
  settingsDb().then((db) => db.transaction('files', 'readwrite').objectStore('files').put(record, 'cmyk-profile'), () => undefined);
}

function iccForget(): void {
  settingsDb().then((db) => db.transaction('files', 'readwrite').objectStore('files').delete('cmyk-profile'), () => undefined);
}

function iccLoad(): Promise<PickedFile | null> {
  return settingsDb().then(
    (db) =>
      new Promise<PickedFile | null>((resolve) => {
        const get = db.transaction('files', 'readonly').objectStore('files').get('cmyk-profile');
        get.onsuccess = () => resolve((get.result as PickedFile | undefined) ?? null);
        get.onerror = () => resolve(null);
      }),
    () => null,
  );
}

/**
 * The fonts downloaded with セットアップ are kept in the browser's own IndexedDB (never a server), each in 1 MiB chunks:
 * store 'chunks' (key [file, n]) and store 'fonts' (key file; written after the last chunk, so a font whose download stopped is
 * not listed and its chunks are removed).
 */
const FONT_CHUNK = 1024 * 1024;

function fontsDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open('ibisoffice-fonts', 1);
    open.onupgradeneeded = () => {
      open.result.createObjectStore('chunks');
      open.result.createObjectStore('fonts');
    };
    open.onerror = () => reject(open.error);
    open.onsuccess = () => resolve(open.result);
  });
}

function idbDone(tx: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  });
}

function idbGet<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

async function fontFetchText(url: string): Promise<string> {
  const response = await fetch(url, { cache: 'no-cache' });
  if (!response.ok) {
    throw new Error(`${url}: ${response.status}`);
  }
  return response.text();
}

async function fontsInstalled(): Promise<{ file: string; size: number }[]> {
  const db = await fontsDb();
  const store = db.transaction('fonts', 'readonly').objectStore('fonts');
  const keys = (await idbGet(store.getAllKeys())) as string[];
  const values = (await idbGet(store.getAll())) as { size: number }[];
  return keys.map((file, i) => ({ file, size: values[i].size }));
}

async function fontDownload(file: string, url: string, size: number): Promise<number> {
  const response = await fetch(url);
  if (!response.ok || !response.body) {
    throw new Error(`${url}: ${response.status}`);
  }
  const db = await fontsDb();
  let chunks = 0;
  let total = 0;
  try {
    const reader = response.body.getReader();
    let pending: Uint8Array[] = [];
    let pendingSize = 0;
    const flush = async (all: boolean) => {
      while (pendingSize >= FONT_CHUNK || (all && pendingSize > 0)) {
        const joined = new Uint8Array(pendingSize);
        let at = 0;
        for (const part of pending) {
          joined.set(part, at);
          at += part.length;
        }
        const chunk = joined.slice(0, Math.min(FONT_CHUNK, pendingSize));
        const rest = joined.slice(chunk.length);
        const tx = db.transaction('chunks', 'readwrite');
        tx.objectStore('chunks').put(chunk, [file, chunks]);
        await idbDone(tx);
        chunks += 1;
        total += chunk.length;
        pending = rest.length > 0 ? [rest] : [];
        pendingSize = rest.length;
      }
    };
    for (;;) {
      const { done, value } = await reader.read();
      if (done) {
        break;
      }
      pending.push(value);
      pendingSize += value.length;
      await flush(false);
    }
    await flush(true);
    if (size > 0 && total !== size) {
      throw new Error(`the size is ${total} bytes, the list says ${size}`);
    }
    const tx = db.transaction('fonts', 'readwrite');
    tx.objectStore('fonts').put({ size: total, chunks }, file);
    await idbDone(tx);
    return total;
  } catch (error) {
    const tx = db.transaction('chunks', 'readwrite');
    tx.objectStore('chunks').delete(IDBKeyRange.bound([file, 0], [file, Number.MAX_SAFE_INTEGER]));
    await idbDone(tx).catch(() => undefined);
    throw error;
  }
}

async function fontsLoad(): Promise<Uint8Array[]> {
  const db = await fontsDb();
  const store = db.transaction('fonts', 'readonly').objectStore('fonts');
  const keys = (await idbGet(store.getAllKeys())) as string[];
  const values = (await idbGet(store.getAll())) as { size: number; chunks: number }[];
  const fonts: Uint8Array[] = [];
  for (let i = 0; i < keys.length; i++) {
    const bytes = new Uint8Array(values[i].size);
    let at = 0;
    for (let n = 0; n < values[i].chunks; n++) {
      // A transaction for each chunk: one held across the awaits could end before the next request.
      const part = (await idbGet(db.transaction('chunks', 'readonly').objectStore('chunks').get([keys[i], n]))) as Uint8Array;
      bytes.set(part, at);
      at += part.length;
    }
    fonts.push(bytes);
  }
  return fonts;
}

/**
 * The file the playground page picked (`?local=1`). It is handed over through
 * the browser's own IndexedDB, never a server, and deleted once read.
 */
function localFile(): Promise<PickedFile | null> {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open('ibisoffice-playground', 1);
    open.onupgradeneeded = () => open.result.createObjectStore('files');
    open.onerror = () => reject(open.error);
    open.onsuccess = () => {
      const tx = open.result.transaction('files', 'readwrite');
      const files = tx.objectStore('files');
      const get = files.get('picked');
      get.onsuccess = () => {
        files.delete('picked');
        resolve((get.result as PickedFile | undefined) ?? null);
      };
      get.onerror = () => reject(get.error);
    };
  });
}

/** The document named by `?open=<url>`, fetched from this site, or the one the playground page picked. */
async function initialFile(): Promise<PickedFile | null> {
  if (new URLSearchParams(location.search).has('local')) {
    return localFile();
  }
  const url = new URLSearchParams(location.search).get('open');
  if (!url) {
    return null;
  }
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`${url}: HTTP ${response.status}`);
  }
  const path = new URL(url, location.href).pathname;
  const name = decodeURIComponent(path.slice(path.lastIndexOf('/') + 1));
  return { name, bytes: new Uint8Array(await response.arrayBuffer()) };
}

const status = document.getElementById('status');
const statusText = document.getElementById('status-text');
const progress = document.getElementById('progress');
const progressFill = progress?.firstElementChild as HTMLElement | null;

/**
 * The wasm's uncompressed size, written next to it by build-web.sh, or 0.
 */
async function wasmSize(): Promise<number> {
  try {
    const response = await fetch(new URL('./pkg/ibis_office_app_bg.wasm.size', import.meta.url));
    return response.ok ? Number((await response.text()).trim()) || 0 : 0;
  } catch {
    return 0;
  }
}

/**
 * Download the app's wasm, filling the progress bar against its size: the
 * size build-web.sh wrote (the bytes the browser hands over are the
 * uncompressed ones, while a server that compresses, as GitHub Pages does,
 * counts the compressed ones in Content-Length), else Content-Length when the
 * transfer is not compressed. The bar stays hidden when neither is known.
 */
async function fetchWasm(): Promise<Response> {
  const url = new URL('./pkg/ibis_office_app_bg.wasm', import.meta.url);
  const [response, size] = await Promise.all([fetch(url), wasmSize()]);
  if (!response.ok) {
    throw new Error(`${url.pathname}: HTTP ${response.status}`);
  }
  const compressed = response.headers.has('Content-Encoding');
  const total = size > 0 ? size : compressed ? 0 : Number(response.headers.get('Content-Length'));
  if (!response.body || !(total > 0)) {
    return response;
  }
  if (progress) {
    progress.style.display = 'block';
  }
  const reader = response.body.getReader();
  const chunks: BlobPart[] = [];
  let loaded = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    chunks.push(value);
    loaded += value.length;
    if (progressFill) {
      progressFill.style.width = `${Math.min(100, (loaded / total) * 100)}%`;
    }
  }
  // Downloaded; compiling and starting the app can take a few more seconds.
  // Fill the bar right away (the download may end short of Content-Length
  // by a few bytes, and the bar's transition would lag behind).
  if (progressFill) {
    progressFill.style.transition = 'none';
    progressFill.style.width = '100%';
  }
  progress?.classList.add('starting');
  if (statusText) {
    statusText.textContent = ja ? 'ibisOffice を起動しています…' : 'Starting ibisOffice…';
  }
  // Let the browser paint the full bar and the new text before the page
  // gets busy compiling and starting the app.
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  return new Response(new Blob(chunks), { headers: { 'Content-Type': 'application/wasm' } });
}

/** Japanese when the viewer chose it on the playground page, or when the browser prefers it. */
function prefersJapanese(): boolean {
  let saved: string | null = null;
  try {
    saved = localStorage.getItem('ibisoffice-lang');
  } catch {
    // Storage may be blocked; fall back to the browser's language.
  }
  return (saved ?? navigator.language).toLowerCase().startsWith('ja');
}

const ja = prefersJapanese();

function showStatus(text: string): void {
  if (statusText) {
    statusText.textContent = text;
  }
  if (progress) {
    progress.style.display = 'none';
  }
}

window.ibisOfficePickFile = () => pickFile();
window.ibisOfficePickIcc = pickIcc;
window.ibisOfficeIccSave = iccSave;
window.ibisOfficeIccForget = iccForget;
window.ibisOfficeIccLoad = iccLoad;
window.ibisOfficeFontFetchText = fontFetchText;
window.ibisOfficeFontsInstalled = fontsInstalled;
window.ibisOfficeFontDownload = fontDownload;
window.ibisOfficeFontsLoad = fontsLoad;
window.ibisOfficeEmbedAction = embedAction;

if (!('gpu' in navigator)) {
  showStatus(ja
    ? 'このブラウザは WebGPU に対応していません。最新の Chrome、Edge、Safari をお使いください。'
    : 'This browser does not support WebGPU. Please use the latest Chrome, Edge or Safari.');
} else {
  try {
    const [, file] = await Promise.all([init({ module_or_path: fetchWasm() }), initialFile()]);
    // Icons load from <base>/assets/icons/*.svg, next to this page (the site
    // may live under a path, as on GitHub Pages). The base must be absolute:
    // the icon loader fetches with reqwest, which rejects relative URLs.
    const base = new URL('.', location.href).href.replace(/\/$/, '');
    const embed = embedOptions();
    set_embed_options(embed.close, embed.download, embed.view, embed.menuBar, embed.toolBar);
    await run(base, file?.name, file?.bytes);
    status?.remove();
  } catch (error) {
    console.error(error);
    showStatus(ja ? `起動できませんでした: ${error}` : `Could not start: ${error}`);
  }
}
