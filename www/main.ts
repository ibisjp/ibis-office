// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// Boots the wasm editor. `pkg/` is produced by scripts/build-web.sh.
import init, {
  run,
  set_embed_options,
  set_font_prompt,
  set_host_mode,
  host_load,
  host_set_read_only,
  host_notify,
  host_snapshot,
  host_snapshot_name,
  host_editor_ready,
} from './pkg/ibis_office_app.js';
import { installHostLoader, resolveOptionUrls, waitForEditor, type HostSession } from './host-loader.js';
import { APP_VERSION } from './build-info.js';
import { loadChunkedWasm, readManifest, type ChunkedEnv, type Manifest } from './chunked-fetch.js';
import { SubsetLoader, subsetsOf, type BlockStore } from './subset-fetch.js';
import type { IbisOfficeEvent } from './ibisoffice-host.js';

/** A file chosen by the user or loaded at startup. */
interface PickedFile {
  name: string;
  bytes: Uint8Array;
}

/**
 * How a page that embeds the editor (a cloud service's preview) sets it up.
 * Read before the editor starts, from `window.ibisOfficeOptions` (set by a
 * script before main.js) over the URL's parameters. See
 * publish/docs/web-embedding.md.
 */
interface EmbedOptions {
  /** `?chunk=<MiB>`: the wasm is served in parts of this size (the page's server says so; `loader-manifest.json` is the truth, GitHub issue #152). */
  chunk?: number;
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
  /** Lead the user to the fonts that can be downloaded (`?fd=0` turns it off). Default: true. */
  fd?: boolean;
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
    /**
     * Called from Rust for the blocks of the fonts (GitHub issue #157, stage 2): what `loader-manifest.json` lists (a JSON text of the fonts, their typefaces and each block's
     * family and ranges), and one block's bytes by the font's id and the block's key. Nothing about a document's characters goes to the server: a block is asked for by name.
     */
    ibisOfficeSubsetsInfo: () => string;
    ibisOfficeSubsetFetch: (fontId: string, key: string) => Promise<Uint8Array>;
    /** Called from Rust for the PDF's fallback font: the one font kept under `file` in IndexedDB, or `null`. */
    ibisOfficeFontLoadOne: (file: string) => Promise<Uint8Array | null>;
    /** Called from Rust when the close or the download button is pressed. */
    ibisOfficeEmbedAction: (kind: string, name: string, bytes?: Uint8Array) => void;
    /** Called from Rust for exports (HTML and the like): the browser saves `bytes` as the file `name`. */
    ibisOfficeSaveFile: (name: string, mime: string, bytes: Uint8Array) => void;
    /** Called from Rust at a print click, before the PDF is made: a blank tab opened while the click still counts (the popup blocker lets it). `null` when the browser refused. */
    ibisOfficeBlankTab: () => Window | null;
    /** Called from Rust when the print's PDF is made: shown in `tab` (a Blob URL), or in a new tab when `tab` is `null`; a tab left over is closed with `bytes` empty. */
    ibisOfficeShowPdf: (tab: Window | null, bytes: Uint8Array) => void;
    /** Called from Rust at a PDF click: asks the browser for the fonts of the viewer's PC (Chrome's queryLocalFonts; its permission prompt needs the click). */
    ibisOfficeLocalFontsStart: () => void;
    /** Called from Rust when the PDF is made: the font file of the family's face nearest the weight and style, `{ bytes, postscriptName }`, or `null` (no such font, no permission, no such browser). */
    ibisOfficeLocalFontFace: (family: string, weight: number, italic: boolean) => Promise<{ bytes: Uint8Array; postscriptName: string } | null>;
    /** Set by the embedding page before main.js. */
    ibisOfficeOptions?: EmbedOptions;
    /** Called from Rust (host mode): a function of the host. `bytes` is the file's contents for saveFile, saveFileAs and printPdf. Resolves to the host's answer (or a HostFailure). */
    ibisOfficeHostCall: (method: string, params: Record<string, unknown>, bytes?: Uint8Array) => Promise<unknown>;
    /** Called from Rust (host mode): a notice to the host (`bytes` for 'download'). */
    ibisOfficeHostEvent: (event: Record<string, unknown>, bytes?: Uint8Array) => void;
  }
}

/** `ibisOfficeOptions.chunk` over `?chunk=<MiB>`: what the page's server says about the parts of the wasm (a number of MiB; 0 means none). */
function chunkArgument(): number | undefined {
  const value = window.ibisOfficeOptions?.chunk ?? new URLSearchParams(location.search).get('chunk');
  if (value === undefined || value === null || value === '') {
    return undefined;
  }
  const mib = Number(value);
  return Number.isFinite(mib) && mib >= 0 ? mib : undefined;
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
function embedOptions(): Required<Omit<EmbedOptions, 'eventTarget' | 'chunk'>> & Pick<EmbedOptions, 'eventTarget'> {
  const params = new URLSearchParams(location.search);
  const js = window.ibisOfficeOptions ?? {};
  const view = js.view ?? flag(params, 'view') ?? false;
  return {
    close: js.close ?? flag(params, 'close') ?? false,
    download: js.download ?? flag(params, 'download') ?? false,
    view,
    menuBar: js.menuBar ?? flag(params, 'menuBar') ?? flag(params, 'menubar') ?? true,
    toolBar: js.toolBar ?? flag(params, 'toolBar') ?? flag(params, 'toolbar') ?? true,
    fd: js.fd ?? flag(params, 'fd') ?? true,
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
  if (hostSession) {
    // A page started for a host: its events, not the window's CustomEvent and the old messages to the parent.
    if (kind === 'close') {
      hostSession.event({ type: 'closeRequested' });
    } else if (kind === 'download' && bytes) {
      hostSession.event({ type: 'download', name, bytes: bytes.slice().buffer as ArrayBuffer });
    }
    return;
  }
  const options = embedOptions();
  const detail = kind === 'download' && bytes ? { name, bytes, blob: new Blob([bytes as BlobPart]) } : {};
  (options.eventTarget ?? window).dispatchEvent(new CustomEvent(`ibisoffice:${kind}`, { detail }));
  // host-lint-allow: the existing embedding protocol (web-embedding.md), sent only to the origin the page was told (`?origin=`); a page started for a host does not get here
  if (window.parent !== window) {
    const origin = parentOrigin(options.targetOrigin);
    if (origin === null) {
      console.warn('ibisOffice: no target origin for the parent window (set ?origin=… or ibisOfficeOptions.targetOrigin); nothing was posted to it.');
      return;
    }
    // The parent gets its own copy of the bytes (transferred), so the event's listeners keep theirs.
    if (kind === 'download' && bytes) {
      const copy = bytes.slice().buffer;
      window.parent.postMessage({ source: 'ibisoffice', type: kind, name, bytes: copy }, origin, [copy]); // host-lint-allow: the existing embedding protocol, to the named origin
    } else {
      window.parent.postMessage({ source: 'ibisoffice', type: kind }, origin); // host-lint-allow: the existing embedding protocol, to the named origin
    }
  }
}

/** Called from Rust for exports: the browser saves `bytes` as the file `name` (a Blob behind a temporary link; nothing is sent anywhere). */
function saveFile(name: string, mime: string, bytes: Uint8Array): void {
  const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: mime }));
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  link.style.display = 'none';
  document.body.appendChild(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 30000);
}

/** A font of the viewer's PC (Chrome's Local Font Access API: `queryLocalFonts`). */
interface LocalFont {
  family: string;
  fullName: string;
  postscriptName: string;
  style: string;
  blob(): Promise<Blob>;
}

let localFontList: Promise<LocalFont[]> | undefined;

/** Ask the browser for the PC's fonts once (a permission prompt the first time). Nothing when the browser has no such API or it is refused. */
function localFonts(): Promise<LocalFont[]> {
  if (!localFontList) {
    const query = (window as unknown as { queryLocalFonts?: () => Promise<LocalFont[]> }).queryLocalFonts;
    localFontList = typeof query === 'function' ? query.call(window).catch(() => []) : Promise.resolve([]);
  }
  return localFontList;
}

/** The weight a face's style name says (Hiragino's `W3`..`W9`, Windows' `Bold`, `Semibold`...), and whether it is italic. */
function styleOf(style: string): { weight: number; italic: boolean } {
  const lower = style.toLowerCase();
  const italic = /italic|oblique/.test(lower);
  const table: [RegExp, number][] = [
    [/thin|hairline|\bw0\b|\bw1\b/, 100],
    [/extra.?light|ultra.?light|\bw2\b/, 200],
    [/light|\bw3\b/, 300],
    [/semi.?bold|demi.?bold|\bw5\b|\bw6\b/, 600],
    [/extra.?bold|ultra.?bold|heavy|black|\bw8\b|\bw9\b/, 800],
    [/bold|\bw7\b/, 700],
    [/medium|\bw4\b/, 500],
  ];
  for (const [pattern, weight] of table) {
    if (pattern.test(lower)) {
      return { weight, italic };
    }
  }
  return { weight: 400, italic };
}

/** The font file of the PC's face of `family` nearest `weight` and `italic`. */
async function localFontFace(family: string, weight: number, italic: boolean): Promise<{ bytes: Uint8Array; postscriptName: string } | null> {
  const wanted = family.toLowerCase();
  const faces = (await localFonts()).filter((f) => f.family.toLowerCase() === wanted || f.fullName.toLowerCase() === wanted);
  let best: LocalFont | null = null;
  let bestScore = Infinity;
  for (const face of faces) {
    const got = styleOf(face.style);
    const score = Math.abs(got.weight - weight) + (got.italic === italic ? 0 : 1000);
    if (score < bestScore) {
      best = face;
      bestScore = score;
    }
  }
  if (!best) {
    return null;
  }
  try {
    return { bytes: new Uint8Array(await (await best.blob()).arrayBuffer()), postscriptName: best.postscriptName };
  } catch {
    return null;
  }
}

/** A blank tab for the print's PDF, opened inside the click (the popup blocker lets it; the PDF takes seconds to make). `opener` is cut. */
function blankTab(): Window | null {
  const tab = window.open('', '_blank');
  if (tab) {
    tab.opener = null;
  }
  return tab;
}

/** The print's PDF in `tab` (or a new tab when `tab` is `null`); with no bytes, a tab left over is closed. The Blob URL is released after ten minutes. */
function showPdf(tab: Window | null, bytes: Uint8Array): void {
  if (bytes.length === 0) {
    tab?.close();
    return;
  }
  const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: 'application/pdf' }));
  if (tab && !tab.closed) {
    tab.location.href = url;
  } else {
    window.open(url, '_blank');
  }
  setTimeout(() => URL.revokeObjectURL(url), 10 * 60 * 1000);
}

/** Show the browser's file picker. Resolves to `null` when cancelled. */
function pickFile(accept?: string): Promise<PickedFile | null> {
  return new Promise((resolve, reject) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = accept ?? '.pptx,.xlsx,.docx,.ppt,.xls,.doc,.md,.markdown,application/vnd.openxmlformats-officedocument.presentationml.presentation,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,application/vnd.openxmlformats-officedocument.wordprocessingml.document';
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
  // (The resources kept beside the fonts, `resource/<kind>/<file>`: transition sounds, `shell/resource_list.rs`, are not fonts.)
  return keys.map((file, i) => ({ file, size: values[i].size })).filter((f) => !f.file.startsWith('resource/'));
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

async function fontLoadOne(file: string): Promise<Uint8Array | null> {
  const db = await fontsDb();
  const info = (await idbGet(db.transaction('fonts', 'readonly').objectStore('fonts').get(file))) as { size: number; chunks: number } | undefined;
  if (!info) {
    return null;
  }
  const bytes = new Uint8Array(info.size);
  let at = 0;
  for (let n = 0; n < info.chunks; n++) {
    const part = (await idbGet(db.transaction('chunks', 'readonly').objectStore('chunks').get([file, n]))) as Uint8Array;
    bytes.set(part, at);
    at += part.length;
  }
  return bytes;
}

async function fontsLoad(): Promise<Uint8Array[]> {
  const db = await fontsDb();
  const store = db.transaction('fonts', 'readonly').objectStore('fonts');
  const keys = (await idbGet(store.getAllKeys())) as string[];
  const values = (await idbGet(store.getAll())) as { size: number; chunks: number }[];
  const fonts: Uint8Array[] = [];
  for (let i = 0; i < keys.length; i++) {
    if (keys[i].startsWith('resource/')) {
      continue;
    }
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

/** Set when the page was started for a host (`?host=1`): `window.IbisOffice` is published here, before anything is awaited. */
const hostLoader = installHostLoader(window, location.search);
let hostSession: HostSession | null = null;

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
/** The blocks of the fonts the manifest lists (null: none, or the host turned font downloads off). Set by `fetchWasm` once the manifest is read. */
let subsetLoader: SubsetLoader | null = null;

/** The blocks fetched before, kept in IndexedDB by their SHA-256 (the store of the page: `ibisoffice-subsets`). */
function subsetStore(): BlockStore {
  const open = (): Promise<IDBDatabase> =>
    new Promise((resolve, reject) => {
      const request = indexedDB.open('ibisoffice-subsets', 1);
      request.onupgradeneeded = () => void request.result.createObjectStore('blocks');
      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve(request.result);
    });
  return {
    get: async (sha256) => {
      const db = await open();
      const found = (await idbGet(db.transaction('blocks', 'readonly').objectStore('blocks').get(sha256))) as Uint8Array | undefined;
      return found ? new Uint8Array(found) : undefined;
    },
    put: async (sha256, bytes) => {
      const db = await open();
      const tx = db.transaction('blocks', 'readwrite');
      tx.objectStore('blocks').put(bytes, sha256);
      await idbDone(tx);
    },
    prune: async (keep) => {
      const db = await open();
      const tx = db.transaction('blocks', 'readwrite');
      const store = tx.objectStore('blocks');
      for (const key of (await idbGet(store.getAllKeys())) as string[]) {
        if (!keep.has(key)) {
          store.delete(key);
        }
      }
      await idbDone(tx);
    },
  };
}

async function fetchWasm(): Promise<Response> {
  const chunkedEnv: ChunkedEnv = {
    fetch: (input, init) => fetch(input, init),
    subtle: globalThis.crypto?.subtle,
    appVersion: APP_VERSION,
    base: new URL('./', import.meta.url),
    chunkArg: chunkArgument(),
    warn: (message) => console.warn(`ibisOffice: ${message}`),
    onProgress: (loaded, total) => {
      if (progress) {
        progress.style.display = 'block';
      }
      if (progressFill) {
        progressFill.style.width = `${Math.min(100, (loaded / total) * 100)}%`;
      }
      if (loaded >= total) {
        progress?.classList.add('starting');
        if (statusText) {
          statusText.textContent = ja ? 'ibisOffice を起動しています…' : 'Starting ibisOffice…';
        }
      }
    },
    decompress: typeof DecompressionStream === 'undefined' ? undefined : (source: ReadableStream<Uint8Array>) => source.pipeThrough(new DecompressionStream('gzip') as unknown as ReadableWritablePair<Uint8Array, Uint8Array>),
  };
  // The manifest is read once: for the wasm in parts, and for the blocks of the fonts.
  const manifest: Manifest | null = await readManifest(chunkedEnv);
  const subsets = subsetsOf(manifest);
  if (subsets) {
    subsetLoader = new SubsetLoader(subsets, { fetch: chunkedEnv.fetch, subtle: chunkedEnv.subtle, base: chunkedEnv.base, store: subsetStore(), warn: chunkedEnv.warn });
    void subsetLoader.prune().catch(() => undefined);
  }
  const chunked = await loadChunkedWasm(chunkedEnv, { manifest });
  if (chunked) {
    return chunked;
  }
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
  if (!hostLoader) {
    // The page of a host shares its storage with the host: it is read only by the playground (host-lint-allow: not in host mode).
    try {
      saved = localStorage.getItem('ibisoffice-lang'); // host-lint-allow: playground only
    } catch {
      // Storage may be blocked; fall back to the browser's language.
    }
  }
  return (saved ?? navigator.language).toLowerCase().startsWith('ja');
}

/** The language of this page's own words (the status line): the host's `locale` once it is known. */
let ja = prefersJapanese();

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
window.ibisOfficeSubsetsInfo = () => {
  // A host that turned font downloads off (`features.fontDownload`) gets none of the blocks.
  if (!subsetLoader || hostSession?.options.features?.fontDownload === false) {
    return '';
  }
  return JSON.stringify({ fonts: subsetLoader.subsets.fonts.map((f) => ({ id: f.id, typeface: f.typeface, blocks: f.blocks.map((b) => ({ key: b.key, family: b.family, ranges: b.ranges, boot: b.boot === true })) })) });
};
window.ibisOfficeSubsetFetch = (fontId, key) => {
  if (!subsetLoader) {
    return Promise.reject(new Error('no blocks of fonts'));
  }
  return subsetLoader.fetchBlock(fontId, key);
};
window.ibisOfficeFontsInstalled = fontsInstalled;
window.ibisOfficeFontDownload = fontDownload;
window.ibisOfficeFontsLoad = fontsLoad;
window.ibisOfficeFontLoadOne = fontLoadOne;
window.ibisOfficeEmbedAction = embedAction;
window.ibisOfficeSaveFile = saveFile;
window.ibisOfficeBlankTab = blankTab;
window.ibisOfficeLocalFontsStart = () => {
  void localFonts();
};
window.ibisOfficeLocalFontFace = localFontFace;
window.ibisOfficeShowPdf = showPdf;
window.ibisOfficeHostCall = async (method, params, bytes) => {
  if (!hostSession) {
    return { ok: false, reason: 'other', message: 'no host' };
  }
  // The view Rust hands over is a copy of its own already (a Vec): the host gets that buffer without another copy.
  const buffer = bytes ? (bytes.buffer as ArrayBuffer).slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) : undefined;
  return hostSession.call(method, params, buffer);
};
window.ibisOfficeHostEvent = (event, bytes) => {
  if (!hostSession) {
    return;
  }
  if (event.type === 'download' && bytes) {
    event.bytes = (bytes.buffer as ArrayBuffer).slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
  }
  hostSession.event(event as unknown as IbisOfficeEvent);
};

/** The host's options as the editor's startup options (the existing `?close=1`, `?download=1` ... and the new `ui` of the options). */
function hostEmbedOptions(session: HostSession): ReturnType<typeof embedOptions> {
  const base = embedOptions();
  const ui = session.options.ui ?? {};
  return {
    ...base,
    close: ui.closeButton ?? base.close,
    download: ui.downloadButton ?? base.download,
    menuBar: ui.menuBar ?? base.menuBar,
    toolBar: ui.toolBar ?? base.toolBar,
    fd: ui.fontSetupPrompt ?? base.fd,
  };
}

if (!('gpu' in navigator)) {
  const message = ja
    ? 'このブラウザは WebGPU に対応していません。最新の Chrome、Edge、Safari をお使いください。'
    : 'This browser does not support WebGPU. Please use the latest Chrome, Edge or Safari.';
  showStatus(message);
  hostLoader?.fail(message);
} else {
  try {
    // The wasm is downloaded while the host connects (it does not wait for the host).
    const wasmReady = init({ module_or_path: fetchWasm() });
    let file: PickedFile | null = null;
    let embed = embedOptions();
    if (hostLoader) {
      [, hostSession] = await Promise.all([wasmReady, hostLoader.session]);
      const locale = hostSession.options.locale;
      if (locale === 'ja' || locale === 'en') {
        ja = locale === 'ja';
      }
      embed = hostEmbedOptions(hostSession);
    } else {
      [, file] = await Promise.all([wasmReady, initialFile()]);
    }
    // Icons load from <base>/assets/icons/*.svg, next to this page (the site
    // may live under a path, as on GitHub Pages). The base must be absolute:
    // the icon loader fetches with reqwest, which rejects relative URLs.
    const base = new URL('.', location.href).href.replace(/\/$/, '');
    set_embed_options(embed.close, embed.download, embed.view, embed.menuBar, embed.toolBar);
    set_font_prompt(embed.fd);
    if (hostSession) {
      set_host_mode(resolveOptionUrls(hostSession.options, document.baseURI), [...hostSession.capabilities]);
    }
    await run(base, file?.name, file?.bytes);
    status?.remove();
    if (hostLoader && hostSession) {
      // `run` returns before the editor window exists (it opens when the GPU is ready): `ready` means load is accepted, so wait for it.
      await waitForEditor(host_editor_ready, ja ? 'エディターの準備が間に合いませんでした' : 'The editor did not become ready in time');
      hostLoader.setHooks({
        load: async (doc) => host_load(doc.name, new Uint8Array(doc.bytes), doc.version, doc.folderId, doc.readOnly === true, doc.readOnlyMessage, doc.bound !== false),
        setReadOnly: (readOnly, message) => host_set_read_only(readOnly, message),
        notify: (notice) => host_notify(notice?.level, notice?.message, notice?.id),
        snapshot: async () => {
          const bytes = host_snapshot();
          return { name: host_snapshot_name(), bytes: bytes.slice().buffer as ArrayBuffer };
        },
      });
      hostSession.event({ type: 'ready' });
    }
  } catch (error) {
    console.error(error);
    const message = ja ? `起動できませんでした: ${error}` : `Could not start: ${error}`;
    showStatus(message);
    hostLoader?.fail(message);
  }
}
