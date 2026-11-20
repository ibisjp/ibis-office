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
export const IBISOFFICE_HOST_PROTOCOL = 1;
/** Direct embedding: the loader (main.js) is in this very page. The editor fills the page; `container` is the element it is for. */
export function mount(container, init) {
    const loader = window.IbisOffice;
    if (!loader?.mount) {
        return Promise.reject(new Error('ibisOffice: the loader (main.js, with ?host=1) is not in this page'));
    }
    return loader.mount(container, init);
}
// ---------- The bridge: attachIframe ----------
/** The names of the functions of `IbisOfficeHost`. */
export const HOST_METHODS = ['listFolder', 'checkSaveTarget', 'openFile', 'saveFile', 'saveFileAs', 'renameFile', 'getFileLocation', 'openUrl', 'printPdf', 'listRecentFiles', 'addRecentFile'];
const PRINT_TAB_URL_LIFETIME_MS = 10 * 60 * 1000;
function failure(message) {
    return { ok: false, reason: 'other', message };
}
function describeError(error) {
    if (error && typeof error.message === 'string') {
        return error.message;
    }
    return String(error);
}
/** The print tab: opened empty right after the click, shown the PDF later, closed on cancel. */
class PrintTab {
    tab = null;
    opened = false;
    begin() {
        this.cancel();
        // `window.open` with 'noopener' would return null; the tab's opener is cut after it is opened.
        const tab = window.open('', '_blank');
        if (tab) {
            try {
                tab.opener = null;
            }
            catch {
                // A browser that does not allow it: the tab is still empty and ours.
            }
        }
        this.tab = tab;
        this.opened = true;
    }
    /** The tab kept by `begin`, handed over (it is no longer ours to close). */
    take() {
        const tab = this.tab;
        this.tab = null;
        this.opened = false;
        return tab;
    }
    cancel() {
        const tab = this.tab;
        this.tab = null;
        this.opened = false;
        if (tab && !tab.closed) {
            tab.close();
        }
    }
    get isOpen() {
        return this.opened;
    }
}
/** The host as ibisOffice sees it: the host's functions, with a default printPdf unless the host gave one or printing is off. */
function prepareHost(host, options) {
    const print = new PrintTab();
    // The host may be a class instance: its methods are on the prototype, which a spread would drop. Each name is read (own or inherited)
    // and bound to the host, so `this` works.
    const wrapped = {};
    if (host) {
        for (const name of HOST_METHODS) {
            const fn = host[name];
            if (typeof fn === 'function') {
                wrapped[name] = fn.bind(host);
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
                }
                else {
                    window.open(url, '_blank', 'noopener,noreferrer');
                }
                return { ok: true };
            }
            catch (error) {
                tab?.close();
                return failure(describeError(error));
            }
        };
    }
    else {
        delete wrapped.printPdf;
    }
    return { host: wrapped, print };
}
/** What `attachIframe` rejects with when it cannot connect: tell the cause by `reason`, not by the message. */
export class IbisOfficeConnectError extends Error {
    reason;
    constructor(reason, message) {
        super(message);
        this.name = 'IbisOfficeConnectError';
        this.reason = reason;
    }
}
function timeoutPromise(ms, error) {
    let timer;
    const promise = new Promise((_, reject) => {
        timer = setTimeout(() => reject(error), ms);
    });
    return { promise, cancel: () => timer !== undefined && clearTimeout(timer) };
}
/**
 * Connect an iframe that serves ibisOffice. Same origin as the host (or no officeOrigin): the host's functions are handed over as
 * they are and nothing is posted. Another origin: one MessageChannel port after hello / connect.
 */
export async function attachIframe(iframe, init) {
    const timeoutMs = (init.connectTimeoutSeconds ?? 10) * 1000;
    const officeOrigin = init.officeOrigin;
    if (officeOrigin === undefined || officeOrigin === location.origin) {
        return attachSameOrigin(iframe, init, timeoutMs);
    }
    return attachCrossOrigin(iframe, init, officeOrigin, timeoutMs);
}
async function attachSameOrigin(iframe, init, timeoutMs) {
    const deadline = Date.now() + timeoutMs;
    let loader;
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
            }
            catch {
                sameOrigin = false;
            }
            if (!blank) {
                if (!sameOrigin) {
                    throw new IbisOfficeConnectError('originMismatch', 'ibisOffice: the iframe is not of the same origin as the host');
                }
                loader = win.IbisOffice;
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
    const wait = timeoutPromise(Math.max(1, deadline - Date.now()), new IbisOfficeConnectError('loaderTimeout', 'ibisOffice: the loader did not become ready in time'));
    try {
        await Promise.race([loader.ready, wait.promise]);
    }
    finally {
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
async function attachCrossOrigin(iframe, init, officeOrigin, timeoutMs) {
    const { host, print } = prepareHost(init.host, init.options);
    const capabilities = HOST_METHODS.filter((name) => typeof host[name] === 'function');
    // 1. hello from the iframe (only its own window, from the origin it is served from).
    await new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
            window.removeEventListener('message', onHello);
            reject(new IbisOfficeConnectError('helloTimeout', 'ibisOffice: no hello from the iframe in time'));
        }, timeoutMs);
        function onHello(event) {
            const data = event.data;
            if (event.origin !== officeOrigin || event.source !== iframe.contentWindow || !data || data.source !== 'ibisoffice' || data.type !== 'hello') {
                return;
            }
            clearTimeout(timer);
            window.removeEventListener('message', onHello);
            if (data.protocol !== IBISOFFICE_HOST_PROTOCOL) {
                reject(new IbisOfficeConnectError('protocolMismatch', `ibisOffice: the protocol differs (host ${IBISOFFICE_HOST_PROTOCOL}, ibisOffice ${String(data.protocol)})`));
            }
            else {
                resolve();
            }
        }
        window.addEventListener('message', onHello);
    });
    // 2. connect, with one end of a channel; from here on the port is the only way.
    const channel = new MessageChannel();
    const port = channel.port1;
    let nextId = 1;
    const pending = new Map();
    let destroyed = false;
    let onEvent = init.onEvent;
    const ready = new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new IbisOfficeConnectError('connectTimeout', 'ibisOffice: the iframe did not answer connect in time')), timeoutMs);
        const first = (event) => {
            const data = event.data;
            if (data?.type === 'ready') {
                clearTimeout(timer);
                resolve();
            }
            else if (data?.type === 'error') {
                clearTimeout(timer);
                reject(new IbisOfficeConnectError('refused', data.message ?? 'ibisOffice: refused'));
            }
        };
        port.addEventListener('message', first);
        port.start();
        setTimeout(() => port.removeEventListener('message', first), timeoutMs + 1000);
    });
    iframe.contentWindow?.postMessage({ source: 'ibisoffice-host', type: 'connect', protocol: IBISOFFICE_HOST_PROTOCOL, options: init.options ?? {}, capabilities }, officeOrigin, [channel.port2]);
    const answer = (id, body, transfer = []) => {
        if (!destroyed) {
            port.postMessage({ id, ...body }, transfer);
        }
    };
    port.addEventListener('message', (event) => {
        const data = event.data;
        if (!data || typeof data !== 'object') {
            return;
        }
        if (data.event) {
            try {
                onEvent?.(data.event);
            }
            catch (error) {
                console.error(error);
            }
            return;
        }
        if (typeof data.method === 'string' && typeof data.id === 'number') {
            // A request from ibisOffice to the host's function.
            const id = data.id;
            const method = data.method;
            if (method === 'printBegin') {
                print.begin();
                answer(id, { result: { ok: true } });
                return;
            }
            if (method === 'printCancel') {
                print.cancel();
                answer(id, { result: { ok: true } });
                return;
            }
            const fn = HOST_METHODS.includes(method) ? host[method] : undefined;
            if (typeof fn !== 'function') {
                answer(id, { error: { message: `no such function: ${String(data.method)}` } });
                return;
            }
            Promise.resolve()
                .then(() => fn.call(host, data.params ?? {}))
                .then((result) => answer(id, { result }), (error) => answer(id, { error: { message: describeError(error) } }));
            return;
        }
        if (typeof data.id === 'number') {
            const entry = pending.get(data.id);
            if (entry) {
                pending.delete(data.id);
                clearTimeout(entry.timer);
                if (data.error) {
                    entry.reject(new Error(data.error.message));
                }
                else {
                    entry.resolve(data.result);
                }
            }
        }
    });
    await ready;
    const request = (method, params, transfer = [], ms = 120000) => new Promise((resolve, reject) => {
        if (destroyed) {
            reject(new Error('ibisOffice: destroyed'));
            return;
        }
        const id = nextId++;
        const timer = setTimeout(() => {
            pending.delete(id);
            reject(new Error(`ibisOffice: ${method} timed out`));
        }, ms);
        pending.set(id, { resolve: resolve, reject, timer });
        port.postMessage({ id, method, params }, transfer);
    });
    return {
        load: async (doc) => {
            await request('load', doc, [doc.bytes], 600000);
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
