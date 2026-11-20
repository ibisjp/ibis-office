// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// Tests of the loader (host-loader.ts) and the bridge (ibisoffice-host.ts), in node with fake windows and iframes
// (scripts/test-web.sh). Both forms: same origin (direct calls) and another origin (hello and one MessageChannel port).
import test from 'node:test';
import assert from 'node:assert/strict';
import { runInNewContext } from 'node:vm';
import { installHostLoader, resolveOptionUrls, waitForEditor, isAllowedUrl, callTimeoutMs, type LoaderWindow, type LoaderHooks } from '../host-loader.js';
import { attachIframe, IbisOfficeConnectError, IBISOFFICE_HOST_PROTOCOL, type IbisOfficeEvent, type IbisOfficeHost } from '../ibisoffice-host.js';

type Listener = (event: { origin: string; source: unknown; data: unknown; ports: readonly MessagePort[] }) => void;

function fakeWindow(): { win: LoaderWindow; posted: { message: unknown; origin: string }[]; listeners: Listener[] } {
  const posted: { message: unknown; origin: string }[] = [];
  const listeners: Listener[] = [];
  const win: LoaderWindow = {
    parent: { postMessage: (message, origin) => void posted.push({ message, origin }) },
    addEventListener: (_type, listener) => void listeners.push(listener as Listener),
  };
  return { win, posted, listeners };
}

const hooks = (log: string[] = []): LoaderHooks => ({
  load: async (doc) => void log.push(`load ${doc.name} ${doc.version} ${String(doc.readOnly)} ${String(doc.bound)} ${doc.bytes.byteLength}${doc.folderId ? ` in ${doc.folderId}` : ''}`),
  setReadOnly: (readOnly, message) => void log.push(`ro ${String(readOnly)} ${String(message)}`),
  notify: (notice) => void log.push(`notify ${notice ? notice.level : 'none'}`),
  snapshot: async () => ({ name: 'a.docx', bytes: new ArrayBuffer(3) }),
});

test('a page without ?host=1 is not a host page', () => {
  const { win } = fakeWindow();
  assert.equal(installHostLoader(win, '', {}), null);
  assert.equal(installHostLoader(win, '?host=0', {}), null);
  assert.equal(win.IbisOffice, undefined);
});

test('the same origin form publishes IbisOffice and sends no message', () => {
  const { win, posted, listeners } = fakeWindow();
  assert.ok(installHostLoader(win, '?host=1'));
  assert.equal(win.IbisOffice?.protocol, IBISOFFICE_HOST_PROTOCOL);
  assert.equal(posted.length, 0);
  assert.equal(listeners.length, 0);
});

test('connect is accepted once; calls reach the host by reference; failures become HostFailure', async () => {
  const { win } = fakeWindow();
  const loader = installHostLoader(win, '?host=1', { timeoutScale: 0.001 })!;
  const events: IbisOfficeEvent[] = [];
  const seen: { bytes?: ArrayBuffer }[] = [];
  const host: IbisOfficeHost = {
    saveFile: async (req) => (seen.push(req), { ok: true, version: 'v2' }),
    renameFile: async () => {
      throw new Error('boom');
    },
    listFolder: () => new Promise(() => undefined),
  };
  const handle = await win.IbisOffice!.connect({ host, options: { locale: 'ja' }, onEvent: (e) => void events.push(e) });
  await assert.rejects(win.IbisOffice!.connect({ host }), /already connected/);
  const session = await loader.session;
  assert.deepEqual([...session.capabilities].sort(), ['listFolder', 'renameFile', 'saveFile']);
  assert.equal(session.options.locale, 'ja');
  const bytes = new ArrayBuffer(8);
  assert.deepEqual(await session.call('saveFile', { baseVersion: 'v1', trigger: 'user' }, bytes), { ok: true, version: 'v2' });
  assert.equal(seen[0].bytes, bytes, 'the bytes go by reference, not copied');
  assert.deepEqual(await session.call('renameFile', { name: 'x', baseVersion: 'v2' }), { ok: false, reason: 'other', message: 'boom' });
  const timedOut = (await session.call('listFolder', { folderId: null })) as { ok: boolean; reason: string };
  assert.equal(timedOut.reason, 'network');
  const missing = (await session.call('getFileLocation', {})) as { ok: boolean; reason: string };
  assert.equal(missing.reason, 'other', 'a function the host did not give');
  session.event({ type: 'dirty', dirty: true });
  assert.deepEqual(events, [{ type: 'dirty', dirty: true }]);
  handle.destroy();
  session.event({ type: 'dirty', dirty: false });
  assert.equal(events.length, 1, 'no event after destroy');
  const after = (await session.call('saveFile', {}, bytes)) as { ok: boolean };
  assert.equal(after.ok, false, 'no call after destroy');
  assert.equal(seen.length, 1);
});

test('calls of one method are one at a time', async () => {
  const { win } = fakeWindow();
  const loader = installHostLoader(win, '?host=1')!;
  const order: string[] = [];
  let release!: () => void;
  const host: IbisOfficeHost = {
    saveFile: async () => {
      order.push('start');
      await new Promise<void>((resolve) => (release = resolve));
      order.push('end');
      return { ok: true, version: 'v' };
    },
  };
  await win.IbisOffice!.connect({ host });
  const session = await loader.session;
  const first = session.call('saveFile', {}, new ArrayBuffer(1));
  const second = session.call('saveFile', {}, new ArrayBuffer(1));
  await new Promise((resolve) => setTimeout(resolve, 5));
  assert.deepEqual(order, ['start']);
  release();
  await first;
  await new Promise((resolve) => setTimeout(resolve, 5));
  assert.deepEqual(order, ['start', 'end', 'start']);
  release();
  await second;
});

test('openUrl lets only https, http and mailto through (read with new URL)', async () => {
  for (const url of ['https://example.com/a', 'http://example.com', 'mailto:a@example.com']) {
    assert.ok(isAllowedUrl(url), url);
  }
  for (const url of ['javascript:alert(1)', 'data:text/html,x', 'file:///etc/passwd', 'blob:https://a/b', 'vbscript:x', 'example.com', ' https://a.com', 'https://a.com/ b', 'JaVaScRiPt:alert(1)', '']) {
    assert.ok(!isAllowedUrl(url), url);
  }
  const { win } = fakeWindow();
  const loader = installHostLoader(win, '?host=1')!;
  const opened: string[] = [];
  await win.IbisOffice!.connect({ host: { openUrl: async (req) => (opened.push(req.url), { ok: true }) } });
  const session = await loader.session;
  assert.deepEqual(await session.call('openUrl', { url: 'https://example.com' }), { ok: true });
  const refused = (await session.call('openUrl', { url: 'javascript:alert(1)' })) as { ok: boolean };
  assert.equal(refused.ok, false);
  assert.deepEqual(opened, ['https://example.com']);
});

test('timeouts: 60 s, 120 s for openFile, 600 s plus 2 s per MB for saves', () => {
  assert.equal(callTimeoutMs('listFolder', 0), 60000);
  assert.equal(callTimeoutMs('openFile', 0), 120000);
  assert.equal(callTimeoutMs('saveFile', 500 * 1024 * 1024), 600000 + 500 * 2000);
  assert.equal(callTimeoutMs('saveFileAs', 0), 600000);
});

test('load waits for the editor, checks its arguments (an ArrayBuffer of another world too), and the other operations pass through', async () => {
  const { win } = fakeWindow();
  const loader = installHostLoader(win, '?host=1')!;
  const handle = await win.IbisOffice!.connect({ host: {} });
  const log: string[] = [];
  const other = runInNewContext('new ArrayBuffer(4)') as ArrayBuffer;
  assert.ok(!(other instanceof ArrayBuffer), 'a buffer of another world is not an instance of ours');
  let loaded = false;
  const pending = handle.load({ name: 'a.docx', bytes: other, version: 'v1', readOnly: true, readOnlyMessage: 'm', folderId: 'd7' }).then(() => (loaded = true));
  await new Promise((resolve) => setTimeout(resolve, 5));
  assert.equal(loaded, false, 'waits until the editor runs');
  loader.setHooks(hooks(log));
  await pending;
  assert.deepEqual(log, ['load a.docx v1 true true 4 in d7']);
  await assert.rejects(handle.load({ name: 'a', bytes: 'x' as unknown as ArrayBuffer, version: 'v' }), /ArrayBuffer/);
  handle.setReadOnly(false, 'free');
  handle.notify({ level: 'warn', message: 'm' });
  handle.notify(null);
  await new Promise((resolve) => setTimeout(resolve, 5));
  assert.deepEqual(log.slice(1), ['ro false free', 'notify warn', 'notify none']);
  assert.equal((await handle.snapshot()).bytes.byteLength, 3);
});

test('an error before the host connects reaches it as an error event', async () => {
  const { win } = fakeWindow();
  const loader = installHostLoader(win, '?host=1')!;
  loader.fail('no webgpu');
  const events: IbisOfficeEvent[] = [];
  await win.IbisOffice!.connect({ host: {}, onEvent: (e) => void events.push(e) });
  assert.deepEqual(events, [{ type: 'error', message: 'no webgpu' }]);
});

// ---- another origin ----

test('another origin: hello goes to the named origin only; no hello without origin= or with *', () => {
  const a = fakeWindow();
  installHostLoader(a.win, '?host=1&origin=https%3A%2F%2Fhost.example');
  assert.equal(a.posted.length, 1);
  assert.equal(a.posted[0].origin, 'https://host.example');
  assert.deepEqual((a.posted[0].message as { type: string; source: string; protocol: number }).type, 'hello');
  assert.equal(a.win.IbisOffice, undefined, 'the same origin entry is not published');
  const b = fakeWindow();
  installHostLoader(b.win, '?host=1&origin=*');
  assert.equal(b.posted.length, 0);
  assert.ok(b.win.IbisOffice);
});

test('another origin: connect from a wrong origin is ignored; the right one gets ready, then RPC both ways', async () => {
  const { win, listeners } = fakeWindow();
  const loader = installHostLoader(win, '?host=1&origin=https%3A%2F%2Fhost.example')!;
  const log: string[] = [];
  loader.setHooks(hooks(log));
  const channel = new MessageChannel();
  const message = { source: 'ibisoffice-host', type: 'connect', protocol: 1, options: { locale: 'en' }, capabilities: ['saveFile'] };
  listeners[0]({ origin: 'https://evil.example', source: null, data: message, ports: [channel.port2] });
  let ready = false;
  const hostSide = new Promise<{ id: number; method: string; params: { bytes: ArrayBuffer } }>((resolve) => {
    channel.port1.onmessage = (event: MessageEvent) => {
      const data = event.data as { type?: string; id?: number; method?: string; params?: { bytes: ArrayBuffer } };
      if (data.type === 'ready') {
        ready = true;
      } else if (data.method) {
        resolve(data as { id: number; method: string; params: { bytes: ArrayBuffer } });
        channel.port1.postMessage({ id: data.id, result: { ok: true, version: 'v9' } });
      } else if ((data as { id?: number }).id === 77) {
        log.push(`answer ${JSON.stringify(data)}`);
      }
    };
  });
  await new Promise((resolve) => setTimeout(resolve, 10));
  assert.equal(ready, false, 'the wrong origin got nothing');
  listeners[0]({ origin: 'https://host.example', source: null, data: message, ports: [channel.port2] });
  const session = await loader.session;
  assert.equal(session.options.locale, 'en');
  assert.deepEqual(session.capabilities, ['saveFile']);
  const result = await session.call('saveFile', { baseVersion: 'v1', trigger: 'user' }, new ArrayBuffer(5));
  assert.deepEqual(result, { ok: true, version: 'v9' });
  const request = await hostSide;
  assert.equal(request.method, 'saveFile');
  assert.equal(request.params.bytes.byteLength, 5);
  assert.equal(ready, true);
  // an operation from the host over the port
  channel.port1.postMessage({ id: 77, method: 'load', params: { name: 'b.xlsx', bytes: new ArrayBuffer(2), version: 'x' } });
  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.ok(log.some((l) => l.startsWith('load b.xlsx x false true 2')), log.join('|'));
  assert.ok(log.some((l) => l.startsWith('answer')), 'the answer returned');
  channel.port1.close();
});

test('another origin: a different protocol is refused on the port', async () => {
  const { win, listeners } = fakeWindow();
  installHostLoader(win, '?host=1&origin=https%3A%2F%2Fhost.example');
  const channel = new MessageChannel();
  const got = new Promise<{ type: string }>((resolve) => (channel.port1.onmessage = (event: MessageEvent) => resolve(event.data as { type: string })));
  listeners[0]({ origin: 'https://host.example', source: null, data: { source: 'ibisoffice-host', type: 'connect', protocol: 99 }, ports: [channel.port2] });
  assert.equal((await got).type, 'error');
  channel.port1.close();
});

// ---- the bridge ----

function setGlobals(origin: string): void {
  (globalThis as unknown as { location: { origin: string } }).location = { origin };
}

test('bridge, same origin: waits for IbisOffice, checks the protocol, connects with the host and a bridge', async () => {
  setGlobals('https://host.example');
  const calls: unknown[] = [];
  const iframe = { contentWindow: { location: { origin: 'https://host.example' } as { origin: string }, IbisOffice: undefined as unknown } };
  setTimeout(() => {
    iframe.contentWindow.IbisOffice = {
      protocol: 1,
      ready: Promise.resolve(),
      connect: async (init: unknown) => (calls.push(init), { destroy: () => undefined }),
    };
  }, 60);
  const host: IbisOfficeHost = { saveFile: async () => ({ ok: true, version: 'v' }) };
  const handle = await attachIframe(iframe as unknown as HTMLIFrameElement, { host, options: { ui: { closeButton: true } } });
  assert.ok(handle);
  const init = calls[0] as { host: IbisOfficeHost; options: unknown; bridge: { printBegin(): void; printCancel(): void } };
  assert.equal(typeof init.host.saveFile, 'function');
  assert.equal(typeof init.host.printPdf, 'function', 'the bridge gives a default printPdf');
  assert.equal(typeof init.bridge.printBegin, 'function');
});

test('bridge, same origin: an iframe still on about:blank (origin "null") is waited for, not taken for another origin (B-1)', async () => {
  setGlobals('https://host.example');
  const calls: unknown[] = [];
  const location = { href: 'about:blank', origin: 'null' };
  const iframe = { contentWindow: { location, IbisOffice: undefined as unknown } };
  setTimeout(() => {
    location.href = 'https://host.example/index.html?host=1';
    location.origin = 'https://host.example';
    iframe.contentWindow.IbisOffice = { protocol: 1, ready: Promise.resolve(), connect: async (init: unknown) => (calls.push(init), { destroy: () => undefined }) };
  }, 80);
  assert.ok(await attachIframe(iframe as unknown as HTMLIFrameElement, { host: {} }));
  assert.equal(calls.length, 1);
  // Still blank at the deadline: the loader did not start in time (not an origin mismatch).
  await assert.rejects(
    attachIframe({ contentWindow: { location: { href: 'about:blank', origin: 'null' } } } as unknown as HTMLIFrameElement, { host: {}, connectTimeoutSeconds: 0.1 }),
    (error: unknown) => error instanceof IbisOfficeConnectError && error.reason === 'loaderTimeout',
  );
  // A real page of another origin is still a mismatch.
  await assert.rejects(
    attachIframe({ contentWindow: { location: { href: 'https://other.example/', origin: 'https://other.example' } } } as unknown as HTMLIFrameElement, { host: {} }),
    (error: unknown) => error instanceof IbisOfficeConnectError && error.reason === 'originMismatch',
  );
});

test('bridge: a host that is a class instance keeps its prototype methods, bound to it (B-3)', async () => {
  setGlobals('https://host.example');
  class Service {
    base = 'b';
    async saveFile(): Promise<{ ok: true; version: string }> {
      return { ok: true, version: this.base };
    }
    async openUrl(): Promise<{ ok: true }> {
      return { ok: true };
    }
    async printPdf(): Promise<{ ok: true }> {
      return { ok: true };
    }
    helper(): void {}
  }
  const calls: { host: IbisOfficeHost }[] = [];
  const loader = { protocol: 1, ready: Promise.resolve(), connect: async (init: { host: IbisOfficeHost }) => (calls.push(init), { destroy: () => undefined }) };
  await attachIframe({ contentWindow: { location: { origin: 'https://host.example' }, IbisOffice: loader } } as unknown as HTMLIFrameElement, { host: new Service() });
  const h = calls[0].host;
  assert.equal(typeof h.saveFile, 'function');
  assert.equal(typeof h.openUrl, 'function');
  assert.equal(typeof h.printPdf, 'function');
  assert.equal((h as Record<string, unknown>).helper, undefined, 'only the interface\'s functions');
  const detached = h.saveFile as unknown as () => Promise<{ version: string }>;
  assert.equal((await detached()).version, 'b', 'this is the host');
});

test('bridge, same origin: no default printPdf when printing is off; a different protocol and a foreign origin fail', async () => {
  setGlobals('https://host.example');
  const calls: { host: IbisOfficeHost }[] = [];
  const loaderOf = (protocol: number) => ({ protocol, ready: Promise.resolve(), connect: async (init: { host: IbisOfficeHost }) => (calls.push(init), { destroy: () => undefined }) });
  await attachIframe({ contentWindow: { location: { origin: 'https://host.example' }, IbisOffice: loaderOf(1) } } as unknown as HTMLIFrameElement, { host: {}, options: { features: { print: false } } });
  assert.equal(calls[0].host.printPdf, undefined);
  await assert.rejects(attachIframe({ contentWindow: { location: { origin: 'https://host.example' }, IbisOffice: loaderOf(2) } } as unknown as HTMLIFrameElement, { host: {} }), (error: unknown) => error instanceof IbisOfficeConnectError && error.reason === 'protocolMismatch' && /protocol differs/.test(error.message));
  assert.equal(calls.length, 1, 'nothing was handed over');
  const foreign = {
    contentWindow: {
      get location(): never {
        throw new Error('cross-origin');
      },
    },
  };
  await assert.rejects(attachIframe(foreign as unknown as HTMLIFrameElement, { host: {} }), (error: unknown) => error instanceof IbisOfficeConnectError && error.reason === 'originMismatch');
  await assert.rejects(attachIframe({ contentWindow: { location: { origin: 'https://host.example' } } } as unknown as HTMLIFrameElement, { host: {}, connectTimeoutSeconds: 0.1 }), (error: unknown) => error instanceof IbisOfficeConnectError && error.reason === 'loaderTimeout' && error.name === 'IbisOfficeConnectError');
});

test('bridge, another origin: hello, connect, then calls both ways through the port', async () => {
  setGlobals('https://host.example');
  const hostListeners: ((event: unknown) => void)[] = [];
  const g = globalThis as unknown as Record<string, unknown>;
  g.window = globalThis;
  g.addEventListener = (_type: string, fn: (event: unknown) => void) => void hostListeners.push(fn);
  g.removeEventListener = (_type: string, fn: (event: unknown) => void) => void hostListeners.splice(hostListeners.indexOf(fn), 1);
  const loaderSide = fakeWindow();
  const iframeWin = {
    postMessage: (data: unknown, _origin: string, ports: MessagePort[]) =>
      loaderSide.listeners[0]({ origin: 'https://host.example', source: null, data, ports }),
  };
  loaderSide.win.parent = {
    postMessage: (message, origin) => {
      assert.equal(origin, 'https://host.example');
      queueMicrotask(() => hostListeners.slice().forEach((fn) => fn({ origin: 'https://office.example', source: iframeWin, data: message })));
    },
  };
  const loader = installHostLoader(loaderSide.win, '?host=1&origin=https%3A%2F%2Fhost.example')!;
  const log: string[] = [];
  loader.setHooks(hooks(log));
  const events: IbisOfficeEvent[] = [];
  const handle = await attachIframe({ contentWindow: iframeWin } as unknown as HTMLIFrameElement, {
    officeOrigin: 'https://office.example',
    host: { saveFile: async (req) => ({ ok: true, version: `v${req.bytes.byteLength}` }), printPdf: undefined },
    options: { locale: 'ja', features: { print: false } },
    onEvent: (e) => void events.push(e),
  });
  const session = await loader.session;
  assert.deepEqual(session.capabilities, ['saveFile'], 'only the functions the host has; no default printPdf when printing is off');
  assert.deepEqual(await session.call('saveFile', { baseVersion: 'a', trigger: 'user' }, new ArrayBuffer(6)), { ok: true, version: 'v6' });
  session.event({ type: 'dirty', dirty: true });
  await handle.load({ name: 'c.pptx', bytes: new ArrayBuffer(7), version: 'q' });
  assert.ok(log.some((l) => l.startsWith('load c.pptx q')));
  assert.equal((await handle.snapshot()).name, 'a.docx');
  await new Promise((resolve) => setTimeout(resolve, 10));
  assert.deepEqual(events, [{ type: 'dirty', dirty: true }]);
  handle.destroy();
});

test('waitForEditor waits until the editor window exists, and gives up after the time (B-2)', async () => {
  let n = 0;
  await waitForEditor(() => ++n >= 4, 'late', 5000);
  assert.equal(n, 4);
  await assert.rejects(waitForEditor(() => false, 'late', 80), /late/);
});

test('K0037: the recent files functions are capabilities; unsaved changes ask the browser before the page is left', async () => {
  const listeners: ((event: { preventDefault(): void; returnValue: unknown }) => void)[] = [];
  const win = {
    parent: { postMessage: () => undefined },
    addEventListener: (type: string, listener: (event: never) => void) => void (type === 'beforeunload' && listeners.push(listener as never)),
  } as unknown as LoaderWindow;
  const loader = installHostLoader(win, '?host=1', { timeoutScale: 0.001 })!;
  assert.equal(listeners.length, 0, 'nothing is listened to before a host connects');
  const host: IbisOfficeHost = {
    listRecentFiles: async () => ({ ok: true, files: [{ name: 'a.docx', url: 'https://example.com/a' }] }),
    addRecentFile: async () => ({ ok: true }),
  };
  await win.IbisOffice!.connect({ host });
  const session = await loader.session;
  assert.deepEqual([...session.capabilities].sort(), ['addRecentFile', 'listRecentFiles']);
  assert.deepEqual(await session.call('listRecentFiles', {}), { ok: true, files: [{ name: 'a.docx', url: 'https://example.com/a' }] });
  assert.equal(listeners.length, 1);
  const ask = (): boolean => {
    let prevented = false;
    listeners[0]({ preventDefault: () => void (prevented = true), returnValue: undefined });
    return prevented;
  };
  assert.equal(ask(), false, 'nothing unsaved: the page is left');
  session.event({ type: 'dirty', dirty: true });
  assert.equal(ask(), true, 'unsaved changes: the browser asks');
  session.event({ type: 'dirty', dirty: false });
  assert.equal(ask(), false, 'saved: the page is left');
});

test('K0037: resources.listUrl is made absolute against the loader page; a bad one is left out', () => {
  const page = 'https://works.example.com/office/0.8.8/index.html?host=1';
  assert.equal(resolveOptionUrls({}, page).resources, undefined);
  assert.equal(resolveOptionUrls({ resources: { listUrl: '/office/resource-list.txt' } }, page).resources?.listUrl, 'https://works.example.com/office/resource-list.txt');
  assert.equal(resolveOptionUrls({ resources: { listUrl: 'sounds/resource-list.txt' } }, page).resources?.listUrl, 'https://works.example.com/office/0.8.8/sounds/resource-list.txt');
  assert.equal(resolveOptionUrls({ resources: { listUrl: 'https://cdn.example.com/r.txt' } }, page).resources?.listUrl, 'https://cdn.example.com/r.txt');
  assert.equal(resolveOptionUrls({ resources: { listUrl: '   ' } }, page).resources, undefined);
  const other = resolveOptionUrls({ locale: 'ja', resources: { listUrl: 'http://[' } }, page);
  assert.equal(other.resources, undefined);
  assert.equal(other.locale, 'ja');
});

test('resources.fontListUrl is made absolute against the loader page like listUrl; each is decided on its own', () => {
  const page = 'https://works.example.com/office/0.8.8/index.html?host=1';
  assert.equal(resolveOptionUrls({ resources: { fontListUrl: '/office/font-list.txt' } }, page).resources?.fontListUrl, 'https://works.example.com/office/font-list.txt');
  assert.equal(resolveOptionUrls({ resources: { fontListUrl: 'fonts/font-list.txt' } }, page).resources?.fontListUrl, 'https://works.example.com/office/0.8.8/fonts/font-list.txt');
  assert.equal(resolveOptionUrls({ resources: { fontListUrl: 'https://cdn.example.com/f.txt' } }, page).resources?.fontListUrl, 'https://cdn.example.com/f.txt');
  assert.equal(resolveOptionUrls({ resources: { fontListUrl: '  ' } }, page).resources, undefined);
  const both = resolveOptionUrls({ resources: { listUrl: 'http://[', fontListUrl: '/f.txt' } }, page);
  assert.equal(both.resources?.listUrl, undefined, 'a broken listUrl is left out');
  assert.equal(both.resources?.fontListUrl, 'https://works.example.com/f.txt', 'and does not take the font list with it');
  const keep = resolveOptionUrls({ resources: { listUrl: '/r.txt', fontListUrl: '/f.txt' } }, page);
  assert.deepEqual(keep.resources, { listUrl: 'https://works.example.com/r.txt', fontListUrl: 'https://works.example.com/f.txt' });
});
