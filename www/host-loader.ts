// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//
// The loader's side of embedding ibisOffice in a host's page (`?host=1`): it accepts the host's functions, and offers the host the
// editor's operations (`IbisOfficeHandle`). The editor itself (wasm) is reached through `LoaderHooks`, which main.ts fills in once it
// runs; this file does not import the wasm, so it is tested on its own (www/test/).
//
// Two forms (see ibisoffice-host.ts and publish/docs/web-embedding.md):
//   - same origin (no `origin=`): `window.IbisOffice.connect` receives the host's functions as they are and calls them directly;
//   - another origin (`origin=<host origin>`): hello, then one MessageChannel port that carries requests and answers.
// Nothing here knows of any one host or of any URL: the only things used are the functions the host handed over.
import {
  IBISOFFICE_HOST_PROTOCOL,
  type HostDocument,
  type HostFailure,
  type IbisOfficeBridgeFunctions,
  type IbisOfficeEvent,
  type IbisOfficeHandle,
  type IbisOfficeHost,
  type IbisOfficeLoader,
  type IbisOfficeMount,
  type IbisOfficeOptions,
} from './ibisoffice-host.js';

/** The editor's operations, set by main.ts when the wasm runs. The handle given to the host waits for them. */
export interface LoaderHooks {
  load(doc: HostDocument): Promise<void>;
  setReadOnly(readOnly: boolean, message?: string): void;
  notify(notice: { level: 'info' | 'warn' | 'error'; message: string; id?: string } | null): void;
  snapshot(): Promise<{ name: string; bytes: ArrayBuffer }>;
}

/** What the editor (Rust) calls: the host's functions as a session. */
export interface HostSession {
  readonly options: IbisOfficeOptions;
  /** The names of the host's functions that exist. */
  readonly capabilities: readonly string[];
  /** Calls a host function (`bytes`: the file's contents for saveFile, saveFileAs and printPdf). Never rejects: a failure is a `HostFailure`. */
  call(method: string, params: Record<string, unknown>, bytes?: ArrayBuffer): Promise<unknown>;
  /** A notice to the host (no answer). */
  event(event: IbisOfficeEvent): void;
}

export interface HostLoader {
  /** Resolves when a host connected. */
  readonly session: Promise<HostSession>;
  /** main.ts gives the editor's operations once the wasm runs (the handle's calls wait for it). */
  setHooks(hooks: LoaderHooks): void;
  /** The editor could not start: the host gets an `error` event. */
  fail(message: string): void;
}

/** The part of `window` the loader uses (a fake in the tests). */
/**
 * The options as the editor reads them: `resources.listUrl` (the list of the files fetched on demand) and `resources.fontListUrl` (the list of the fonts) made absolute against `base`
 * (the loader's page), so that a relative address of the host's own server works. Each is decided on its own: one that cannot be made absolute is left out (the editor then uses the
 * public repository's list for it). Nothing else changes.
 */
export function resolveOptionUrls(options: IbisOfficeOptions, base: string): IbisOfficeOptions {
  const resources = options.resources;
  if (resources === undefined) {
    return options;
  }
  const resolve = (value: unknown): string | undefined => {
    if (typeof value !== 'string' || value.trim() === '') {
      return undefined;
    }
    try {
      return new URL(value.trim(), base).href;
    } catch {
      return undefined;
    }
  };
  const { resources: _dropped, ...rest } = options;
  void _dropped;
  const { listUrl: _list, fontListUrl: _font, ...others } = resources;
  void _list;
  void _font;
  const listUrl = resolve(resources.listUrl);
  const fontListUrl = resolve(resources.fontListUrl);
  const kept = { ...others, ...(listUrl === undefined ? {} : { listUrl }), ...(fontListUrl === undefined ? {} : { fontListUrl }) };
  return Object.keys(kept).length === 0 ? rest : { ...rest, resources: kept };
}

export interface LoaderWindow {
  parent: { postMessage(message: unknown, targetOrigin: string): void };
  addEventListener(type: 'message', listener: (event: { origin: string; source: unknown; data: unknown; ports: readonly MessagePort[] }) => void): void;
  /** Optional: the tests' fake windows have no such event. */
  addEventListener(type: 'beforeunload', listener: (event: { preventDefault(): void; returnValue: unknown }) => void): void;
  IbisOffice?: IbisOfficeLoader;
}

export interface LoaderTuning {
  /** Multiplies every timeout (the tests make them short). Default 1. */
  timeoutScale?: number;
}

/** Waits until `ready()` is true (the editor window exists, so the editor's operations act); rejects with `message` after `timeoutMs`. */
export async function waitForEditor(ready: () => boolean, message: string, timeoutMs = 60000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!ready()) {
    if (Date.now() > deadline) {
      throw new Error(message);
    }
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
}

const toString = Object.prototype.toString;

/** An ArrayBuffer of this or another JavaScript world (`instanceof` is false across worlds). */
export function isArrayBuffer(value: unknown): value is ArrayBuffer {
  return toString.call(value) === '[object ArrayBuffer]';
}

function describeError(error: unknown): string {
  if (error && typeof (error as { message?: unknown }).message === 'string') {
    return (error as { message: string }).message;
  }
  return String(error);
}

function failure(reason: HostFailure['reason'], message: string): HostFailure {
  return { ok: false, reason, message };
}

/**
 * Whether `url` may be opened by `openUrl`: its scheme (read with `new URL`, not by comparing the start of the string) is https, http
 * or mailto, with no blank or control character in it. The host checks again; the host's check is the one that counts.
 */
export function isAllowedUrl(url: unknown): boolean {
  if (typeof url !== 'string' || url.length === 0 || url.length > 8192) {
    return false;
  }
  // eslint-disable-next-line no-control-regex
  if (/[\u0000- \u007f-\u009f]/.test(url)) {
    return false;
  }
  try {
    const protocol = new URL(url).protocol;
    return protocol === 'https:' || protocol === 'http:' || protocol === 'mailto:';
  } catch {
    return false;
  }
}

/** How long a call may take, in milliseconds (the design's 60 s, saves 600 s plus 2 s per MB, openFile 120 s). */
export function callTimeoutMs(method: string, byteLength: number): number {
  switch (method) {
    case 'saveFile':
    case 'saveFileAs':
      return 600000 + Math.ceil(byteLength / (1024 * 1024)) * 2000;
    case 'openFile':
      return 120000;
    default:
      return 60000;
  }
}

type Invoke = (method: string, params: Record<string, unknown>, bytes?: ArrayBuffer) => Promise<unknown>;

/** One call at a time of each method (a double press of save waits for the first), with a timeout. */
class Session implements HostSession {
  private chains = new Map<string, Promise<unknown>>();
  private closed = false;

  constructor(
    readonly options: IbisOfficeOptions,
    readonly capabilities: readonly string[],
    private invoke: Invoke,
    private emit: (event: IbisOfficeEvent) => void,
    private scale: number,
  ) {}

  call(method: string, params: Record<string, unknown>, bytes?: ArrayBuffer): Promise<unknown> {
    const previous = this.chains.get(method) ?? Promise.resolve();
    const run = previous.then(() => this.callNow(method, params, bytes));
    this.chains.set(method, run.catch(() => undefined));
    return run;
  }

  private async callNow(method: string, params: Record<string, unknown>, bytes?: ArrayBuffer): Promise<unknown> {
    if (this.closed) {
      return failure('other', 'ibisOffice: the host is disconnected');
    }
    if (method !== 'printBegin' && method !== 'printCancel' && !this.capabilities.includes(method)) {
      return failure('other', `ibisOffice: the host has no ${method}`);
    }
    if (method === 'openUrl' && !isAllowedUrl(params.url)) {
      return failure('other', 'ibisOffice: this link cannot be opened');
    }
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timeout = new Promise<HostFailure>((resolve) => {
      timer = setTimeout(() => resolve(failure('network', `ibisOffice: ${method} timed out`)), callTimeoutMs(method, bytes?.byteLength ?? 0) * this.scale);
    });
    try {
      const result = await Promise.race([this.invoke(method, params, bytes), timeout]);
      if (result === null || typeof result !== 'object') {
        return failure('other', `ibisOffice: ${method} returned no result`);
      }
      return result;
    } catch (error) {
      // An exception of the host's function is a failure of reason 'other' (the user is shown its message).
      return failure('other', describeError(error));
    } finally {
      clearTimeout(timer);
    }
  }

  event(event: IbisOfficeEvent): void {
    if (!this.closed) {
      this.emit(event);
    }
  }

  close(): void {
    this.closed = true;
  }
}

/** The loader inside the iframe. Returns `null` when the page was not started for a host (`?host=1` is absent). */
export function installHostLoader(win: LoaderWindow, search: string, tuning: LoaderTuning = {}): HostLoader | null {
  const params = new URLSearchParams(search);
  const hostFlag = (params.get('host') ?? '').toLowerCase();
  if (!params.has('host') || hostFlag === '0' || hostFlag === 'false' || hostFlag === 'no' || hostFlag === 'off') {
    return null;
  }
  const scale = tuning.timeoutScale ?? 1;
  let resolveSession!: (session: HostSession) => void;
  const session = new Promise<HostSession>((resolve) => (resolveSession = resolve));

  let hooks: LoaderHooks | null = null;
  let hooksWaiters: ((hooks: LoaderHooks) => void)[] = [];
  const whenHooks = (): Promise<LoaderHooks> => (hooks ? Promise.resolve(hooks) : new Promise((resolve) => hooksWaiters.push(resolve)));
  let current: Session | null = null;
  let connected = false;
  let pendingFailure: string | null = null;
  let sendError: ((message: string) => void) | null = null;

  /** The handle's operations, for both forms. `destroy` is given by the form. */
  const operations = {
    async load(doc: HostDocument): Promise<void> {
      if (!doc || typeof doc !== 'object' || typeof doc.name !== 'string' || typeof doc.version !== 'string' || !isArrayBuffer(doc.bytes)) {
        throw new Error('ibisOffice: load needs { name, bytes (an ArrayBuffer), version }');
      }
      const h = await whenHooks();
      await h.load({
        name: doc.name,
        bytes: doc.bytes,
        version: doc.version,
        readOnly: doc.readOnly === true,
        readOnlyMessage: typeof doc.readOnlyMessage === 'string' ? doc.readOnlyMessage : undefined,
        bound: doc.bound !== false,
        folderId: typeof doc.folderId === 'string' ? doc.folderId : undefined,
      });
    },
    setReadOnly(readOnly: boolean, message?: string): void {
      void whenHooks().then((h) => h.setReadOnly(readOnly === true, typeof message === 'string' ? message : undefined));
    },
    notify(notice: { level: 'info' | 'warn' | 'error'; message: string; id?: string } | null): void {
      const valid = notice && typeof notice.message === 'string' && (notice.level === 'info' || notice.level === 'warn' || notice.level === 'error');
      void whenHooks().then((h) => h.notify(valid ? { level: notice.level, message: notice.message, id: typeof notice.id === 'string' ? notice.id : undefined } : null));
    },
    async snapshot(): Promise<{ name: string; bytes: ArrayBuffer }> {
      return (await whenHooks()).snapshot();
    },
  };

  // Unsaved changes: closing the tab (⌘W and Ctrl+W are the browser's, a page does not get them) or leaving the page asks the browser's own question (K0037).
  // The listener is added when a host connects.
  let unsaved = false;
  let guardInstalled = false;
  const installUnloadGuard = (): void => {
    if (guardInstalled) {
      return;
    }
    guardInstalled = true;
    win.addEventListener('beforeunload', (event) => {
      if (unsaved && typeof event.preventDefault === 'function') {
        event.preventDefault();
        event.returnValue = '';
      }
    });
  };
  const noteDirty = (event: IbisOfficeEvent): void => {
    if (event.type === 'dirty') {
      unsaved = event.dirty;
    }
  };

  const origin = params.get('origin') ?? '';
  const crossOrigin = origin !== '' && origin !== '*';

  if (!crossOrigin) {
    // ---- same origin: the host calls `connect` with its functions ----
    win.IbisOffice = {
      protocol: IBISOFFICE_HOST_PROTOCOL,
      ready: Promise.resolve(),
      // Direct embedding (the host in this very page): the editor fills the page, so the container is only the host's own handle on it.
      mount(_container: unknown, init: IbisOfficeMount): Promise<IbisOfficeHandle> {
        return (win.IbisOffice as IbisOfficeLoader).connect(init);
      },
      connect(init: IbisOfficeMount & { bridge?: IbisOfficeBridgeFunctions }): Promise<IbisOfficeHandle> {
        if (connected) {
          return Promise.reject(new Error('ibisOffice: already connected'));
        }
        connected = true;
        installUnloadGuard();
        let host: IbisOfficeHost | undefined = init.host;
        let onEvent = init.onEvent;
        let bridge = init.bridge;
        const capabilities = (['listFolder', 'checkSaveTarget', 'openFile', 'saveFile', 'saveFileAs', 'renameFile', 'getFileLocation', 'openUrl', 'printPdf', 'listRecentFiles', 'addRecentFile'] as const).filter(
          (name) => typeof host?.[name] === 'function',
        );
        const invoke: Invoke = async (method, p, bytes) => {
          if (method === 'printBegin' || method === 'printCancel') {
            bridge?.[method]();
            return { ok: true };
          }
          const fn = (host as unknown as Record<string, ((req: unknown) => Promise<unknown>) | undefined>)[method];
          if (!fn) {
            throw new Error(`no such function: ${method}`);
          }
          // The bytes go by reference: no copy, and nothing transferred (the same memory of this world).
          return fn.call(host, bytes ? { ...p, bytes } : p);
        };
        const options = init.options && typeof init.options === 'object' ? init.options : {};
        current = new Session(options, capabilities, invoke, (event) => { noteDirty(event); onEvent?.(event); }, scale);
        sendError = (message) => onEvent?.({ type: 'error', message });
        resolveSession(current);
        if (pendingFailure !== null) {
          sendError(pendingFailure);
        }
        const handle: IbisOfficeHandle = {
          ...operations,
          destroy(): void {
            // The host's functions are let go, so they do not stay in the iframe's memory.
            current?.close();
            current = null;
            host = undefined;
            onEvent = undefined;
            bridge = undefined;
            sendError = null;
          },
        };
        return Promise.resolve(handle);
      },
    };
  } else {
    // ---- another origin: hello, then one port ----
    win.addEventListener('message', (event) => {
      const data = event.data as { source?: string; type?: string; protocol?: number; options?: IbisOfficeOptions; capabilities?: string[] } | null;
      if (event.origin !== origin || !data || typeof data !== 'object' || data.source !== 'ibisoffice-host' || data.type !== 'connect') {
        return;
      }
      const port = event.ports[0];
      if (!port || connected) {
        return;
      }
      if (data.protocol !== IBISOFFICE_HOST_PROTOCOL) {
        port.postMessage({ type: 'error', message: `ibisOffice: the protocol differs (host ${String(data.protocol)}, ibisOffice ${IBISOFFICE_HOST_PROTOCOL})` });
        return;
      }
      connected = true;
      installUnloadGuard();
      let nextId = 1;
      const waiting = new Map<number, { resolve(value: unknown): void; reject(error: Error): void }>();
      const invoke: Invoke = (method, p, bytes) =>
        new Promise((resolve, reject) => {
          const id = nextId++;
          waiting.set(id, { resolve, reject });
          // The bytes go with the request, transferred (no copy).
          port.postMessage({ id, method, params: bytes ? { ...p, bytes } : p }, bytes ? [bytes] : []);
        });
      const capabilities = Array.isArray(data.capabilities) ? data.capabilities.filter((name) => typeof name === 'string') : [];
      const options = data.options && typeof data.options === 'object' ? data.options : {};
      current = new Session(options, capabilities, invoke, (e) => {
        noteDirty(e);
        const transfer = e.type === 'download' ? [e.bytes] : [];
        port.postMessage({ event: e }, transfer);
      }, scale);
      sendError = (message) => port.postMessage({ event: { type: 'error', message } });
      port.onmessage = (message: MessageEvent) => {
        const m = message.data as { id?: number; method?: string; params?: Record<string, unknown>; result?: unknown; error?: { message: string } } | null;
        if (!m || typeof m.id !== 'number') {
          return;
        }
        if (typeof m.method === 'string') {
          const id = m.id;
          const reply = (body: { result?: unknown; error?: { message: string } }, transfer: Transferable[] = []): void => port.postMessage({ id, ...body }, transfer);
          const p = m.params ?? {};
          const run: Promise<unknown> =
            m.method === 'load'
              ? operations.load(p as unknown as HostDocument)
              : m.method === 'setReadOnly'
                ? Promise.resolve(operations.setReadOnly(p.readOnly === true, p.message as string | undefined))
                : m.method === 'notify'
                  ? Promise.resolve(operations.notify((p.notice as never) ?? null))
                  : m.method === 'snapshot'
                    ? operations.snapshot()
                    : m.method === 'destroy'
                      ? Promise.resolve(void (current?.close(), (current = null), (sendError = null), port.close()))
                      : Promise.reject(new Error(`no such operation: ${m.method}`));
          run.then(
            (result) => {
              const transfer = result && typeof result === 'object' && isArrayBuffer((result as { bytes?: unknown }).bytes) ? [(result as { bytes: ArrayBuffer }).bytes] : [];
              try {
                reply({ result }, transfer);
              } catch {
                // A closed port after destroy.
              }
            },
            (error) => {
              try {
                reply({ error: { message: describeError(error) } });
              } catch {
                // A closed port after destroy.
              }
            },
          );
          return;
        }
        const entry = waiting.get(m.id);
        if (entry) {
          waiting.delete(m.id);
          if (m.error) {
            entry.reject(new Error(m.error.message));
          } else {
            entry.resolve(m.result);
          }
        }
      };
      port.postMessage({ type: 'ready' });
      resolveSession(current);
      if (pendingFailure !== null) {
        sendError(pendingFailure);
      }
    });
    // hello: to the page that embeds (the origin named by `origin=` only; never `*`).
    // host-lint-allow: the other-origin form's hello, sent to the origin named by `origin=` only (never `*`)
    win.parent.postMessage({ source: 'ibisoffice', type: 'hello', protocol: IBISOFFICE_HOST_PROTOCOL, capabilities: ['load', 'setReadOnly', 'notify', 'snapshot', 'destroy'] }, origin);
  }

  return {
    session,
    setHooks(h: LoaderHooks): void {
      hooks = h;
      const waiters = hooksWaiters;
      hooksWaiters = [];
      for (const waiter of waiters) {
        waiter(h);
      }
    },
    fail(message: string): void {
      pendingFailure = message;
      sendError?.(message);
    },
  };
}
