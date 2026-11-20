// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//
// The host side of embedding ibisOffice: the types of the interface between the page that embeds the editor (the "host")
// and ibisOffice, and the bridge (`attachIframe`) a host uses to connect an iframe that runs ibisOffice.
// scripts/build-web.sh compiles this file to ibisoffice-host.js and ibisoffice-host.d.ts, which ship in the Web archive.
// The loader inside the iframe (host-loader.ts, main.ts) speaks the other half of the protocol.
// See publish/docs/web-embedding.md.
//
// This file knows nothing of any one host: the host is whatever page implements `IbisOfficeHost`.

/** The version of the agreement between the host and ibisOffice. ibisOffice refuses a connection whose version differs. */
export const IBISOFFICE_HOST_PROTOCOL = 1 as const;

// ---------- Options (host -> ibisOffice) ----------

export interface IbisOfficeOptions {
  /** The display language. Default 'auto' (the browser's). */
  locale?: 'ja' | 'en' | 'auto';
  /** The colour scheme. Default 'auto' (the OS's). */
  theme?: 'light' | 'dark' | 'auto';
  /** The menu bar, the toolbar, the close button and the like. */
  ui?: {
    menuBar?: boolean; // default true
    toolBar?: boolean; // default true
    closeButton?: boolean; // default false. Pressed, it sends the closeRequested event.
    downloadButton?: boolean; // default false. Pressed, it sends the download event (the existing ?download=1).
    fontSetupPrompt?: boolean; // default true (the existing ?fd)
    startWindow?: boolean; // default true. false: no start view (スタート画面) while there is no document (the window is blank until load), and no start window item in the menus.
  };
  /** Narrows the features down. An item left out is true (usable). */
  features?: {
    /** Show 'open from local storage' and 'save to local (download)'. */
    localFiles?: boolean;
    /** Printing. Usable only when the host gives printPdf (the bridge gives a default). */
    print?: boolean;
    /** Export (saved to the host's storage with saveFileAs). false: all off. An array: only those formats (e.g. ['pdf']). */
    export?: boolean | string[];
    /** Opening links to the outside. */
    openLinks?: boolean;
    /** Downloading the extra fonts (the setup). */
    fontDownload?: boolean;
  };
  /** Auto save. 0 or absent: off. On, saveFile is called with trigger 'auto' this many seconds after a change. */
  autoSaveSeconds?: number;
  /** A watermark drawn over the document. Not saved in the document's file. */
  watermark?: { text: string; opacity?: number /* 0 to 1, default 0.12 */; includeInPrint?: boolean /* default true */ };
  /** The user who works (the author of comments and tracked changes, the status display). */
  user?: { id?: string; name: string };
  /** Where 'source code (AGPL)...' of the help goes: the source of a modified version that is served (AGPL section 13). Default: the ibisOffice project's page. */
  sourceUrl?: string;
  /**
   * The files ibisOffice fetches when it needs them (the sounds of the slide transitions: `resource-list.txt`; the fonts: `font-list.txt`). Absent: the public repository's lists. The host may serve the lists and
   * the files itself (a company's own server; ibisWorks).
   */
  resources?: {
    /**
     * The address of `resource-list.txt`: https, or a loopback address (http://127.0.0.1, http://localhost); a relative address is resolved against the page of the loader. The list's own lines
     * may give a file's `url` (and `license_url`) relative too, against the list's address (`sounds/a/a.ogg`, `/office/sounds/a/a.ogg`).
     */
    listUrl?: string;
    /**
     * The address of `font-list.txt` (the fonts セットアップ offers to download, and the font the PDF falls back to): https, or a loopback address (http://127.0.0.1, http://localhost); a
     * relative address is resolved against the page of the loader. The list's own lines give a font file's `url` (and `license_url`), relative too against the list's address
     * (`fonts/a/a.ttf`, `/office/fonts/a/a.ttf`). Absent, or not an address the editor may fetch: the public repository's list. `features.fontDownload: false` still turns the downloads off.
     */
    fontListUrl?: string;
  };
  /** The host's start view: a page that runs ibisOffice with no document (its 'Open...' is the host's file window). The logo at the left end of the menu bar opens it in a new tab (through openUrl). Absent: the logo does nothing. */
  startUrl?: string;
}

// ---------- The document (host -> ibisOffice) ----------

export interface HostDocument {
  /** The file name, with its extension. */
  name: string;
  /** The file's contents. */
  bytes: ArrayBuffer;
  /** The version of these contents. ibisOffice does not look inside; it hands it back as the next saveFile / renameFile's baseVersion. */
  version: string;
  /** Open read-only. */
  readOnly?: boolean;
  /** The text of the information bar when read-only (the host decides the language). */
  readOnlyMessage?: string;
  /** Whether the document is tied to the host's file. false (default true): opened as a local document. */
  bound?: boolean;
  /**
   * The id of the folder the file is in. Optional: the file window (open, save as, export) starts in that folder (without it, in the folder used
   * last in this run, else the top). ibisOffice does not look inside; it hands it back as `folderId` of `listFolder`.
   */
  folderId?: string;
}

// ---------- What the host implements (ibisOffice -> host) ----------

export type HostErrorReason =
  | 'locked' // somebody else is editing it
  | 'conflict' // somebody else saved first (the version is old)
  | 'readOnly' // a read-only document
  | 'noPermission'
  | 'notFound' // deleted or moved
  | 'exists' // a file of that name exists
  | 'tooLarge'
  | 'unauthorized' // the session expired
  | 'network' // cannot communicate
  | 'other';

export interface HostFailure {
  ok: false;
  reason: HostErrorReason;
  /** The text shown to the user (the host decides the language). Absent: ibisOffice's own. */
  message?: string;
  /** Lead the user to 'save as'. Absent: true for locked/conflict/readOnly/notFound/noPermission. */
  offerSaveAs?: boolean;
}

export interface HostFolderRef {
  id: string;
  name: string;
}

/**
 * The kind of a root (what `listFolder(null)` returns). ibisOffice uses it for the icon only and keeps the order the host gave; what a kind
 * means is the host's. 'personal': one's own; 'shared': what all share; 'joined': a container of the places one takes part in (it need not
 * be a real folder); 'pinned': favourites; 'other': anything else (an unknown value is read as 'other').
 */
export type HostRootKind = 'personal' | 'shared' | 'joined' | 'pinned' | 'other';

export interface HostFolder {
  id: string;
  name: string;
  /** Whether it has sub folders (the tree's expand mark). */
  hasSubFolders: boolean;
  /** Whether a file can be saved in this folder. */
  canWrite: boolean;
  /** On the roots (the folders of listFolder(null)): their kind. Absent on an ordinary folder. */
  rootKind?: HostRootKind;
}

/** A recently opened file as the host keeps it. */
export interface HostRecentFile {
  /** The file name shown (with its extension: the icon is chosen from it). */
  name: string;
  /** The address of the page that opens the file (what `getFileLocation` gave). */
  url: string;
}

export interface HostFileEntry {
  id: string;
  name: string;
  /** Bytes. */
  size: number;
  /** ISO 8601. */
  updatedAt: string;
  /** The display name of who updated it. */
  updatedBy?: string;
}

export interface HostFolderListing {
  /** The folder seen; null when folderId is null (the top). */
  folder: (HostFolder & { path: HostFolderRef[] }) | null;
  /** The sub folders of this call's page, when include.folders. */
  folders: HostFolder[];
  /** The files of this call's page, when include.files. */
  files: HostFileEntry[];
  /** When there is more: the value for the next page. ibisOffice does not look inside; it passes it as the next cursor. */
  nextCursor?: string;
  /**
   * A continuation that takes the host long to make (a slow calculation). ibisOffice does not use it by itself: it shows 続きの表示 at the
   * end of the tree's node and of the list, and asks (the same folderId, cursor = this value) only when the user presses it. The host returns
   * what is quick first and leaves the rest behind this value. With nextCursor too, ibisOffice takes nextCursor to its end first.
   */
  deferredCursor?: string;
}

/**
 * The functions of a storage SaaS that ibisOffice uses, as a type (K0041). Whatever the SaaS (its web API, its sign-in, its permissions, how
 * its folders are made, what is slow) is the host adapter's business; ibisOffice sees only this. `IbisOfficeHost` is this and the rest.
 *
 * What an adapter keeps to:
 * - listFolder: folderId null is the list of roots (each with rootKind), a root's children are asked with the root's id. ids and cursors are
 *   opaque strings of up to 255 characters. The order is the host's. path starts with the root the folder belongs to. canWrite is true only
 *   where saving is possible. A page is at most `limit` items. deferredCursor is the slow continuation, nextCursor the quick one.
 * - checkSaveTarget (optional): answers without the paged list. Without it, ibisOffice reads the rest of the pages of the folder and looks
 *   for the same name there.
 * - saveFileAs: the host judges conflicts and holds the locks; a failure is a HostFailure with its reason.
 * The smallest one (a SaaS where a folder is chosen and a file saved) is listFolder and saveFileAs. ibisOffice turns on what is given.
 */
export type HostStorage = Pick<
  IbisOfficeHost,
  'listFolder' | 'checkSaveTarget' | 'openFile' | 'saveFile' | 'saveFileAs' | 'renameFile' | 'getFileLocation' | 'listRecentFiles' | 'addRecentFile'
>;

export interface IbisOfficeHost {
  /**
   * One page of the children of a folder (null: the top). Used by the file window (open, save as, export).
   * There can be hundreds of thousands of folders, so ibisOffice never takes them all: the tree's node, when opened, takes only the
   * sub folders of that folder; the right list takes only the folder chosen. Taken in pages with cursor and limit.
   * A host without it gets no file window: the local file picker and the download are used as before.
   */
  listFolder?(req: {
    folderId: string | null;
    purpose: 'open' | 'saveAs' | 'export';
    include: { folders: boolean; files: boolean };
    /** The extensions (lower case, no dot) of the files listed. Absent: all. */
    extensions?: string[];
    cursor?: string;
    /** The most items of a page (the host may give fewer). ibisOffice passes 200. */
    limit: number;
  }): Promise<HostFolderListing | HostFailure>;

  /**
   * Before save as / export: whether the folder can be written and whether a file of that name is there. Optional: a host whose list is
   * paged can answer without it being read to the end. Without it ibisOffice reads the remaining pages of the folder and compares the names.
   */
  checkSaveTarget?(req: {
    folderId: string;
    name: string;
    purpose: 'saveAs' | 'export';
  }): Promise<{ ok: true; canWrite: boolean; existing?: { id: string; name: string } } | HostFailure>;

  /** A file was chosen in the file window. The host opens it (closes the current document, moves the lock, and gives the new document with load). */
  openFile?(req: { fileId: string }): Promise<{ ok: true } | HostFailure>;

  /** Save (Ctrl+S). bytes: the contents with the edits, in the file's own format. */
  saveFile?(req: { bytes: ArrayBuffer; baseVersion: string; trigger: 'user' | 'auto' | 'close' }): Promise<{ ok: true; version: string } | HostFailure>;

  /**
   * Save as, export and make a copy: a new file in a folder of the host (overwriteFileId: over that file).
   * kind 'saveAs': on success the document is tied to the new file (replaced by the returned file / version).
   * 'export' and 'copy': the document's tie does not change.
   */
  saveFileAs?(req: {
    kind: 'saveAs' | 'export' | 'copy';
    folderId: string;
    name: string;
    bytes: ArrayBuffer;
    overwriteFileId?: string;
  }): Promise<{ ok: true; file: { id: string; name: string }; version: string } | HostFailure>;

  /** Rename (the extension does not change). On success the new name and version. */
  renameFile?(req: { name: string; baseVersion: string }): Promise<{ ok: true; name: string; version: string } | HostFailure>;

  /** 'Get the path': a URL (or a string for the place) the host decides. ibisOffice only copies it to the clipboard. */
  getFileLocation?(): Promise<{ ok: true; url: string } | HostFailure>;

  /**
   * The files opened lately, newest first (ファイル > 最近使ったファイル; ibisOffice shows at most 20). The host keeps them wherever it decides (ibisWorks: the browser's
   * localStorage). A line opens through the host's `openUrl` with the file's `url`, so the host gives the address of the page that opens the file.
   */
  listRecentFiles?(): Promise<{ ok: true; files: HostRecentFile[] } | HostFailure>;

  /**
   * A file was opened (or saved under a new name) in the editor: the host puts it first in its list. `url` is what `getFileLocation` returned for it; the same `url` is not listed
   * twice. Called only when the host also gave `getFileLocation`.
   */
  addRecentFile?(req: HostRecentFile): Promise<{ ok: true } | HostFailure>;

  /**
   * Open an external URL (the help's 'source code (AGPL)', a link in a document). The host opens a new tab.
   * The check of the scheme and `noopener,noreferrer` are the host's, in one place; ibisOffice does not open it itself.
   * A host without it: opening links is off.
   */
  openUrl?(req: { url: string }): Promise<{ ok: true } | HostFailure>;

  /**
   * Print. ibisOffice makes the PDF and hands it over; the host opens it in a new tab (the browser's PDF viewer prints it).
   * tab: the empty tab the bridge opened right after the click on print (so that the pop-up blocker does not stop it while the PDF is made).
   * The host shows the PDF there, or (tab null) opens a tab itself. A host without it: the bridge's default is used.
   */
  printPdf?(req: { name: string; bytes: ArrayBuffer; tab: WindowProxy | null }): Promise<{ ok: true } | HostFailure>;
}

// ---------- Notices (ibisOffice -> host; no answer) ----------

export type IbisOfficeEvent =
  | { type: 'ready' } // started (load is accepted)
  | { type: 'loaded'; name: string; kind: 'docx' | 'xlsx' | 'pptx' }
  | { type: 'dirty'; dirty: boolean } // whether there are unsaved changes changed
  | { type: 'documentChanged'; source: 'local' | 'host' | 'new' } // the document was replaced (opened from local, a new one)
  | { type: 'saved'; trigger: 'user' | 'auto' | 'close' } // saved (the host's saveFile returned ok)
  | { type: 'closeRequested' } // the close button (the question about unsaved changes is done)
  | { type: 'download'; name: string; bytes: ArrayBuffer } // the download button (existing)
  | { type: 'error'; message: string }; // a failure to start or load

// ---------- What ibisOffice offers (host -> ibisOffice) ----------

export interface IbisOfficeHandle {
  /** Open a document (replacing the current one). */
  load(doc: HostDocument): Promise<void>;
  /** Switch read-only on or off. message: the text of the information bar. */
  setReadOnly(readOnly: boolean, message?: string): void;
  /** A notice from the host to the user (the information bar). It replaces the previous one of the same level; null clears. */
  notify(notice: { level: 'info' | 'warn' | 'error'; message: string; id?: string } | null): void;
  /** The current contents (the same bytes as a save). For a host that wants to check before closing. */
  snapshot(): Promise<{ name: string; bytes: ArrayBuffer }>;
  /** Clean up (call before removing the iframe). */
  destroy(): void;
}

export interface IbisOfficeMount {
  options?: IbisOfficeOptions;
  host?: IbisOfficeHost;
  onEvent?(event: IbisOfficeEvent): void;
}

/** The functions the bridge hands ibisOffice for printing (same origin form; the cross origin form uses the messages of the same names). */
export interface IbisOfficeBridgeFunctions {
  printBegin(): void;
  printCancel(): void;
}

/** What the loader inside the iframe publishes as `window.IbisOffice` (same origin form). */
export interface IbisOfficeLoader {
  protocol: number;
  /** Resolves when the loader accepts connect (it does not wait for the wasm download). */
  ready: Promise<void>;
  /** Accepts the host's functions once. */
  connect(init: IbisOfficeMount & { bridge?: IbisOfficeBridgeFunctions }): Promise<IbisOfficeHandle>;
  /** Direct embedding (the loader and the host in the same page). */
  mount?(container: HTMLElement, init: IbisOfficeMount): Promise<IbisOfficeHandle>;
}

declare global {
  interface Window {
    IbisOffice?: IbisOfficeLoader;
  }
}

/** Direct embedding: the loader (main.js) is in this very page. The editor fills the page; `container` is the element it is for. */
export function mount(container: HTMLElement, init: IbisOfficeMount): Promise<IbisOfficeHandle> {
  const loader = window.IbisOffice;
  if (!loader?.mount) {
    return Promise.reject(new Error('ibisOffice: the loader (main.js, with ?host=1) is not in this page'));
  }
  return loader.mount(container, init);
}

// ---------- The bridge: attachIframe ----------

/** The names of the functions of `IbisOfficeHost`. */
export const HOST_METHODS = ['listFolder', 'checkSaveTarget', 'openFile', 'saveFile', 'saveFileAs', 'renameFile', 'getFileLocation', 'openUrl', 'printPdf', 'listRecentFiles', 'addRecentFile'] as const;
export type HostMethod = (typeof HOST_METHODS)[number];

const PRINT_TAB_URL_LIFETIME_MS = 10 * 60 * 1000;

export type AttachIframeInit = IbisOfficeMount & {
  /**
   * The origin the iframe is served from. Absent, or the host's own: the same origin form (direct calls of functions).
   * Another origin: the postMessage form, used for targetOrigin and for checking what is received (e.g. 'https://ibisoffice.example.com').
   */
  officeOrigin?: string;
  /** Seconds to wait for the loader (same origin form) or for hello (cross origin form). Default 10. */
  connectTimeoutSeconds?: number;
};

function failure(message: string): HostFailure {
  return { ok: false, reason: 'other', message };
}

function describeError(error: unknown): string {
  if (error && typeof (error as { message?: unknown }).message === 'string') {
    return (error as { message: string }).message;
  }
  return String(error);
}

/** The print tab: opened empty right after the click, shown the PDF later, closed on cancel. */
class PrintTab {
  private tab: WindowProxy | null = null;
  private opened = false;

  begin(): void {
    this.cancel();
    // `window.open` with 'noopener' would return null; the tab's opener is cut after it is opened.
    const tab = window.open('', '_blank');
    if (tab) {
      try {
        tab.opener = null;
      } catch {
        // A browser that does not allow it: the tab is still empty and ours.
      }
    }
    this.tab = tab;
    this.opened = true;
  }

  /** The tab kept by `begin`, handed over (it is no longer ours to close). */
  take(): WindowProxy | null {
    const tab = this.tab;
    this.tab = null;
    this.opened = false;
    return tab;
  }

  cancel(): void {
    const tab = this.tab;
    this.tab = null;
    this.opened = false;
    if (tab && !tab.closed) {
      tab.close();
    }
  }

  get isOpen(): boolean {
    return this.opened;
  }
}

/** The host as ibisOffice sees it: the host's functions, with a default printPdf unless the host gave one or printing is off. */
function prepareHost(host: IbisOfficeHost | undefined, options: IbisOfficeOptions | undefined): { host: IbisOfficeHost; print: PrintTab } {
  const print = new PrintTab();
  // The host may be a class instance: its methods are on the prototype, which a spread would drop. Each name is read (own or inherited)
  // and bound to the host, so `this` works.
  const wrapped: IbisOfficeHost = {};
  if (host) {
    for (const name of HOST_METHODS) {
      const fn = (host as unknown as Record<string, unknown>)[name];
      if (typeof fn === 'function') {
        (wrapped as unknown as Record<string, unknown>)[name] = (fn as (...a: unknown[]) => unknown).bind(host);
      }
    }
  }
  if (options?.features?.print !== false) {
    const own = host?.printPdf?.bind(host);
    wrapped.printPdf = async (req) => {
      const tab = print.take();
      try {
        if (own) {
          return await own({ name: req.name, bytes: req.bytes, tab });
        }
        const url = URL.createObjectURL(new Blob([req.bytes], { type: 'application/pdf' }));
        setTimeout(() => URL.revokeObjectURL(url), PRINT_TAB_URL_LIFETIME_MS);
        if (tab && !tab.closed) {
          tab.location.href = url;
        } else {
          window.open(url, '_blank', 'noopener,noreferrer');
        }
        return { ok: true };
      } catch (error) {
        tab?.close();
        return failure(describeError(error));
      }
    };
  } else {
    delete wrapped.printPdf;
  }
  return { host: wrapped, print };
}

/** Why `attachIframe` could not connect. */
export type ConnectErrorReason =
  | 'originMismatch'   // the iframe is not of the origin the call says (it went to another page, or it is not the same-origin form)
  | 'loaderTimeout'    // the loader (index.html?host=1) did not publish window.IbisOffice, or did not become ready, in time
  | 'helloTimeout'     // another origin: no hello from the iframe in time
  | 'connectTimeout'   // another origin: the iframe did not answer connect in time
  | 'protocolMismatch' // the host and ibisOffice speak different versions of the interface
  | 'refused';         // ibisOffice refused the connection (its message is the error's message)

/** What `attachIframe` rejects with when it cannot connect: tell the cause by `reason`, not by the message. */
export class IbisOfficeConnectError extends Error {
  readonly reason: ConnectErrorReason;
  constructor(reason: ConnectErrorReason, message: string) {
    super(message);
    this.name = 'IbisOfficeConnectError';
    this.reason = reason;
  }
}

function timeoutPromise<T>(ms: number, error: Error): { promise: Promise<T>; cancel(): void } {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const promise = new Promise<T>((_, reject) => {
    timer = setTimeout(() => reject(error), ms);
  });
  return { promise, cancel: () => timer !== undefined && clearTimeout(timer) };
}

/**
 * Connect an iframe that serves ibisOffice. Same origin as the host (or no officeOrigin): the host's functions are handed over as
 * they are and nothing is posted. Another origin: one MessageChannel port after hello / connect.
 */
export async function attachIframe(iframe: HTMLIFrameElement, init: AttachIframeInit): Promise<IbisOfficeHandle> {
  const timeoutMs = (init.connectTimeoutSeconds ?? 10) * 1000;
  const officeOrigin = init.officeOrigin;
  if (officeOrigin === undefined || officeOrigin === location.origin) {
    return attachSameOrigin(iframe, init, timeoutMs);
  }
  return attachCrossOrigin(iframe, init, officeOrigin, timeoutMs);
}

async function attachSameOrigin(iframe: HTMLIFrameElement, init: AttachIframeInit, timeoutMs: number): Promise<IbisOfficeHandle> {
  const deadline = Date.now() + timeoutMs;
  let loader: IbisOfficeLoader | undefined;
  // The loader publishes window.IbisOffice before it waits for anything, so it is looked for until it is there (the iframe's 'load'
  // may have fired before this call, or be later than the script).
  for (;;) {
    const win = iframe.contentWindow;
    if (win) {
      let sameOrigin = false;
      let blank = false;
      try {
        // Right after the iframe is made it shows about:blank (its origin reads "null"): the real page is not loaded yet, so wait for it.
        blank = win.location.href === 'about:blank';
        // Another origin throws when it is read: the iframe went to another page (or this is not the same origin form).
        sameOrigin = win.location.origin === location.origin;
      } catch {
        sameOrigin = false;
      }
      if (!blank) {
        if (!sameOrigin) {
          throw new IbisOfficeConnectError('originMismatch', 'ibisOffice: the iframe is not of the same origin as the host');
        }
        loader = (win as Window).IbisOffice;
        if (loader) {
          break;
        }
      }
    }
    if (Date.now() > deadline) {
      throw new IbisOfficeConnectError('loaderTimeout', 'ibisOffice: the loader did not start in time');
    }
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  if (loader.protocol !== IBISOFFICE_HOST_PROTOCOL) {
    throw new IbisOfficeConnectError('protocolMismatch', `ibisOffice: the protocol differs (host ${IBISOFFICE_HOST_PROTOCOL}, ibisOffice ${loader.protocol})`);
  }
  const wait = timeoutPromise<never>(Math.max(1, deadline - Date.now()), new IbisOfficeConnectError('loaderTimeout', 'ibisOffice: the loader did not become ready in time'));
  try {
    await Promise.race([loader.ready, wait.promise]);
  } finally {
    wait.cancel();
  }
  const { host, print } = prepareHost(init.host, init.options);
  return loader.connect({
    options: init.options,
    host,
    onEvent: init.onEvent,
    bridge: { printBegin: () => print.begin(), printCancel: () => print.cancel() },
  });
}

interface PortRequest {
  id: number;
  method: string;
  params?: Record<string, unknown>;
}

interface PortResponse {
  id: number;
  result?: unknown;
  error?: { message: string };
}

async function attachCrossOrigin(iframe: HTMLIFrameElement, init: AttachIframeInit, officeOrigin: string, timeoutMs: number): Promise<IbisOfficeHandle> {
  const { host, print } = prepareHost(init.host, init.options);
  const capabilities = HOST_METHODS.filter((name) => typeof host[name] === 'function');

  // 1. hello from the iframe (only its own window, from the origin it is served from).
  await new Promise<void>((resolve, reject) => {
    const timer = setTimeout(() => {
      window.removeEventListener('message', onHello);
      reject(new IbisOfficeConnectError('helloTimeout', 'ibisOffice: no hello from the iframe in time'));
    }, timeoutMs);
    function onHello(event: MessageEvent): void {
      const data = event.data as { source?: string; type?: string; protocol?: number } | null;
      if (event.origin !== officeOrigin || event.source !== iframe.contentWindow || !data || data.source !== 'ibisoffice' || data.type !== 'hello') {
        return;
      }
      clearTimeout(timer);
      window.removeEventListener('message', onHello);
      if (data.protocol !== IBISOFFICE_HOST_PROTOCOL) {
        reject(new IbisOfficeConnectError('protocolMismatch', `ibisOffice: the protocol differs (host ${IBISOFFICE_HOST_PROTOCOL}, ibisOffice ${String(data.protocol)})`));
      } else {
        resolve();
      }
    }
    window.addEventListener('message', onHello);
  });

  // 2. connect, with one end of a channel; from here on the port is the only way.
  const channel = new MessageChannel();
  const port = channel.port1;
  let nextId = 1;
  const pending = new Map<number, { resolve(value: unknown): void; reject(error: Error): void; timer: ReturnType<typeof setTimeout> }>();
  let destroyed = false;
  let onEvent = init.onEvent;

  const ready = new Promise<void>((resolve, reject) => {
    const timer = setTimeout(() => reject(new IbisOfficeConnectError('connectTimeout', 'ibisOffice: the iframe did not answer connect in time')), timeoutMs);
    const first = (event: MessageEvent): void => {
      const data = event.data as { type?: string; message?: string } | null;
      if (data?.type === 'ready') {
        clearTimeout(timer);
        resolve();
      } else if (data?.type === 'error') {
        clearTimeout(timer);
        reject(new IbisOfficeConnectError('refused', data.message ?? 'ibisOffice: refused'));
      }
    };
    port.addEventListener('message', first);
    port.start();
    setTimeout(() => port.removeEventListener('message', first), timeoutMs + 1000);
  });
  iframe.contentWindow?.postMessage(
    { source: 'ibisoffice-host', type: 'connect', protocol: IBISOFFICE_HOST_PROTOCOL, options: init.options ?? {}, capabilities },
    officeOrigin,
    [channel.port2],
  );

  const answer = (id: number, body: { result?: unknown; error?: { message: string } }, transfer: Transferable[] = []): void => {
    if (!destroyed) {
      port.postMessage({ id, ...body }, transfer);
    }
  };

  port.addEventListener('message', (event: MessageEvent) => {
    const data = event.data as (Partial<PortRequest> & Partial<PortResponse> & { event?: IbisOfficeEvent }) | null;
    if (!data || typeof data !== 'object') {
      return;
    }
    if (data.event) {
      try {
        onEvent?.(data.event);
      } catch (error) {
        console.error(error);
      }
      return;
    }
    if (typeof data.method === 'string' && typeof data.id === 'number') {
      // A request from ibisOffice to the host's function.
      const id = data.id;
      const method = data.method as HostMethod;
      if (method === 'printBegin' as string) {
        print.begin();
        answer(id, { result: { ok: true } });
        return;
      }
      if (method === 'printCancel' as string) {
        print.cancel();
        answer(id, { result: { ok: true } });
        return;
      }
      const fn = (HOST_METHODS as readonly string[]).includes(method) ? host[method] : undefined;
      if (typeof fn !== 'function') {
        answer(id, { error: { message: `no such function: ${String(data.method)}` } });
        return;
      }
      Promise.resolve()
        .then(() => (fn as (req: unknown) => Promise<unknown>).call(host, data.params ?? {}))
        .then(
          (result) => answer(id, { result }),
          (error) => answer(id, { error: { message: describeError(error) } }),
        );
      return;
    }
    if (typeof data.id === 'number') {
      const entry = pending.get(data.id);
      if (entry) {
        pending.delete(data.id);
        clearTimeout(entry.timer);
        if (data.error) {
          entry.reject(new Error(data.error.message));
        } else {
          entry.resolve(data.result);
        }
      }
    }
  });

  await ready;

  const request = <T>(method: string, params: unknown, transfer: Transferable[] = [], ms = 120000): Promise<T> =>
    new Promise<T>((resolve, reject) => {
      if (destroyed) {
        reject(new Error('ibisOffice: destroyed'));
        return;
      }
      const id = nextId++;
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error(`ibisOffice: ${method} timed out`));
      }, ms);
      pending.set(id, { resolve: resolve as (value: unknown) => void, reject, timer });
      port.postMessage({ id, method, params }, transfer);
    });

  return {
    load: async (doc) => {
      await request<void>('load', doc, [doc.bytes], 600000);
    },
    setReadOnly: (readOnly, message) => void request('setReadOnly', { readOnly, message }).catch(console.error),
    notify: (notice) => void request('notify', { notice }).catch(console.error),
    snapshot: () => request('snapshot', {}, [], 600000),
    destroy: () => {
      if (destroyed) {
        return;
      }
      void request('destroy', {}, [], 5000).catch(() => undefined);
      destroyed = true;
      onEvent = undefined;
      print.cancel();
      for (const entry of pending.values()) {
        clearTimeout(entry.timer);
      }
      pending.clear();
      port.close();
    },
  };
}
