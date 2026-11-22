# Embedding the browser version

**English** | [日本語](web-embedding-ja.md)

The browser version of ibisOffice is a set of static files (a loader, a WebAssembly module and a few assets). You put them on your own web server, load them in an `iframe` of your page, and hand the page's own functions (where files are listed, opened and saved) to ibisOffice. ibisOffice draws and edits Word (.docx), Excel (.xlsx) and PowerPoint (.pptx) documents in the browser; your system keeps the files, the sign-in and the permissions. The editor runs in the visitor's browser and calls only the functions you give it.

This page, in order:

1. [Get the browser version](#get-the-browser-version): download and check the archive.
2. [Minimal setup](#minimal-setup): where to put the files, what the server must do, the smallest page that opens a document.
3. [Options and host interface reference](#options-and-host-interface-reference): every option, function and event.
4. [Turning the buttons on](#turning-the-buttons-on), [What the page receives](#what-the-page-receives) and [the host interface](#letting-the-page-keep-the-files-the-host-interface): the Close and Download buttons, the events, and the host in detail.
5. [Pinning a version](#pinning-a-version) and [serving the wasm in parts](#serving-the-wasm-in-parts-for-a-cdn-that-limits-the-size-of-a-file).

## Get the browser version

The quickest way is `npm install @ibisjp/ibis-office` (see [Install from npm](#install-from-npm)). The same files are also attached to every release as an archive.

Every release carries the browser version as an archive. The browser version of [v0.8.9](https://github.com/ibisjp/ibis-office/releases/tag/v0.8.9) is these three files:

| File | What it is |
|---|---|
| [`ibisOffice-0.8.9-Web.zip`](https://github.com/ibisjp/ibis-office/releases/download/v0.8.9/ibisOffice-0.8.9-Web.zip) | The browser version |
| [`ibisOffice-0.8.9-Web.zip.sha256`](https://github.com/ibisjp/ibis-office/releases/download/v0.8.9/ibisOffice-0.8.9-Web.zip.sha256) | SHA-256 of the archive |
| [`ibisOffice-0.8.9-Web.files.sha256`](https://github.com/ibisjp/ibis-office/releases/download/v0.8.9/ibisOffice-0.8.9-Web.files.sha256) | SHA-256 of each file inside the archive, by path |

The files of a version are at `https://github.com/ibisjp/ibis-office/releases/download/v<version>/<file>`. A release's files do not change once it is published.

Check what you fetched:

```sh
V=<version>
BASE=https://github.com/ibisjp/ibis-office/releases/download/v$V
curl -LO $BASE/ibisOffice-$V-Web.zip
curl -LO $BASE/ibisOffice-$V-Web.zip.sha256
curl -LO $BASE/ibisOffice-$V-Web.files.sha256

shasum -a 256 -c ibisOffice-$V-Web.zip.sha256        # macOS (Linux: sha256sum -c ...)
mkdir app && unzip -q ibisOffice-$V-Web.zip -d app
(cd app && shasum -a 256 -c ../ibisOffice-$V-Web.files.sha256)   # every line must say OK
```

On Windows PowerShell, compare `(Get-FileHash ibisOffice-$V-Web.zip -Algorithm SHA256).Hash` with the hash in the `.sha256` file (case-insensitive).

The archive must contain `ibisoffice-host.js`, the file your page imports in the next section. List it with `unzip -l ibisOffice-$V-Web.zip`; an archive of an older version, from before the host interface, does not have it.

## Install from npm

The same files are on npm as [`@ibisjp/ibis-office`](https://www.npmjs.com/package/@ibisjp/ibis-office). The version of the package is the version of the release (`0.9.0` carries the browser version of v0.9.0). It holds the host loader (what your page imports) and the editor's files (`editor/`).

```sh
npm install @ibisjp/ibis-office          # or one line in package.json: "@ibisjp/ibis-office": "^0.9.0"
npx ibis-office-copy public/ibisoffice   # copies the editor into public/ibisoffice/<version>/
```

Serve the copied folder as described in [Minimal setup](#minimal-setup). Your page imports `attachIframe` from `@ibisjp/ibis-office` (with a bundler or an import map), or from the copied folder's `ibisoffice-host.js`. To take a newer release, run `npm update @ibisjp/ibis-office` and `npx ibis-office-copy` again; a folder per version keeps the files cacheable. `--flat` copies into the folder itself. The package installs no scripts and `ibis-office-copy` uses only Node's standard library. It is under the same licence as the archive (see [Licence](#licence)).

## Minimal setup

### What is in the archive

The archive is the folder to serve, starting at its top:

| File | What it is |
|---|---|
| `index.html` | The page that runs the editor. Your `iframe` points at it with `?host=1`. It has no inline script. |
| `boot.js`, `main.js`, `host-loader.js`, `chunked-fetch.js`, `subset-fetch.js`, `build-info.js` | The loader. Keep them in the same directory as `index.html`. |
| `pkg/` | The WebAssembly module (`ibis_office_app_bg.wasm`, tens of megabytes), its glue (`ibis_office_app.js`) and `ibis_office_app_bg.wasm.size` (the size the download's progress bar counts to). |
| `assets/` | Icons and the borders of page frames (with `artborders/ECMA-NOTICE.txt`). |
| `ibisoffice-host.js`, `ibisoffice-host.d.ts` | What your page imports: `attachIframe` and the types of the options, the functions and the events. |
| `license.md` | The third-party licences of the browser version. |

### Where to put it

Put the unzipped folder anywhere your web server serves static files, and **serve it as it is** (do not rename or move files inside it). A folder per version is the simplest layout:

```text
/ibisoffice/0.8.9/index.html
/ibisoffice/0.8.9/main.js
/ibisoffice/0.8.9/ibisoffice-host.js
/ibisoffice/0.8.9/pkg/ibis_office_app_bg.wasm
...
```

Keep `license.md` and `assets/artborders/ECMA-NOTICE.txt` when you serve the files.

### What the server must do

| Setting | What to do |
|---|---|
| HTTPS | Serve your page and the editor over HTTPS (`http://localhost` also works while developing). The editor draws with WebGPU, which browsers offer to secure pages only: Chrome, Edge and Safari 26 or later. |
| Same origin | Serve the editor from the same origin as your page (the form this page shows). Another origin works too, through messages; see the cross-origin form in [the host interface](#letting-the-page-keep-the-files-the-host-interface). |
| `.wasm` type | Send `.wasm` as `application/wasm`, `.js` as `text/javascript`. |
| Compression | Send `.wasm` and `.js` compressed (gzip or Brotli). The wasm is tens of megabytes; compressed it is about a quarter of that. |
| Caching | The file names do not carry a hash. Under a folder per version (above), cache them for a long time (`Cache-Control: public, max-age=31536000, immutable`) and give the next version another folder. Under a fixed folder, send `Cache-Control: no-cache` with an `ETag`, so a new version is picked up. |
| `COOP` / `COEP` headers | Not needed. The editor runs on one thread and does not use `SharedArrayBuffer`. |
| Content-Security-Policy | If you set one, `script-src 'self' 'wasm-unsafe-eval'` is enough (no `unsafe-eval`), and `frame-ancestors 'self'` for the page. The editor's `index.html` has one inline `style` element, so a `style-src` must allow it. Allow in `connect-src` the addresses of the next section if you use them. |
| `iframe` | Do not give the `iframe` a `sandbox` attribute. |

### What the editor fetches from outside

Two kinds of files are not in the archive. They are fetched only when a document needs them:

- **Sounds of slide transitions** (`resource-list.txt` and the sounds it names). By default the editor reads `https://raw.githubusercontent.com/ibisjp/ibis-office/main/docs/resource-list.txt` and the sounds on `https://ibisjp.github.io/ibis-office/sounds/`. To serve them yourself, copy `docs/resource-list.txt` and the `sounds/<id>/` folders (each with its `LICENSE`) to your server, change each line's `url` and `license_url` to a path beside the list (for example `sounds/applause/applause.ogg`), and give the list's address in the option `resources.listUrl` (a relative address is resolved against the page of the loader). The file's `size` (and `sha256`, when the line has one) is checked after the download.
- **Fonts** that セットアップ offers to download (and the prompt that leads to it). By default the list is `https://raw.githubusercontent.com/ibisjp/ibis-office/main/docs/font-list.txt`. To serve it yourself, copy `docs/font-list.txt` and the font files it names (each in its folder, with its licence file) to your server, change each line's `url` and `license_url` to a path relative to the list's directory (for example `fonts/ACGJMincho/acgjm.ttf`), and give the list's address in `resources.fontListUrl`; mind each font's licence before you serve it (the licence is in its folder). The file's `size` (and `sha256`, when the line has one) is checked after the download. Switch the prompt off with `ui.fontSetupPrompt: false` and the downloads with `features.fontDownload: false`. To serve the fonts under the SIL Open Font License in blocks, see [fonts in blocks](#fonts-in-blocks-the-browser-version-takes-only-what-a-document-needs); a page with `features.fontDownload: false` takes no blocks.

If a transition sound or a font cannot be fetched, the editor goes on without it.

### The smallest page that opens a document

Your page puts the editor in an `iframe`, connects with `attachIframe` and gives the document to `load`. Here the page's server keeps files under `/api/files/42` (an example; any storage works) and the editor is served under `/ibisoffice/0.8.9/`:

```html
<!doctype html>
<meta charset="utf-8">
<iframe id="office" src="/ibisoffice/0.8.9/index.html?host=1"
        style="width:100%; height:100vh; border:0"></iframe>
<script type="module">
  import { attachIframe } from '/ibisoffice/0.8.9/ibisoffice-host.js';

  const handle = await attachIframe(document.getElementById('office'), {
    options: { locale: 'en', user: { name: 'Taro Yamada' }, ui: { closeButton: true, startWindow: false } },
    host: {
      // Called for Save. Return the new version, or why it failed.
      async saveFile({ bytes, baseVersion }) {
        const r = await fetch('/api/files/42', { method: 'PUT', body: bytes, headers: { 'If-Match': baseVersion } });
        if (r.status === 412) return { ok: false, reason: 'conflict' };
        return r.ok ? { ok: true, version: r.headers.get('ETag') } : { ok: false, reason: 'other' };
      },
    },
    onEvent(event) {
      if (event.type === 'closeRequested') location.href = '/files';
    },
  });

  const r = await fetch('/api/files/42');
  await handle.load({ name: 'report.docx', bytes: await r.arrayBuffer(), version: r.headers.get('ETag') });
</script>
```

What happens: `attachIframe` waits for the loader in the `iframe`, then hands it the options and the functions. `load` opens the document; the user edits it; Save calls your `saveFile` with the document's bytes in its own format (the same bytes as File > Save writes). ibisOffice never learns the URL or the sign-in behind `saveFile`; it hands back the `version` you gave it as `baseVersion` at the next save.

A function you do not give is a feature you do not offer: without `listFolder` there is no file window (the local file picker and the download are used), without `saveFile` Save asks where to save. If `attachIframe` cannot connect it throws an `IbisOfficeConnectError` whose `reason` tells why (see [the host interface](#letting-the-page-keep-the-files-the-host-interface)).

## Options and host interface reference

The types are in `ibisoffice-host.d.ts`. This section lists them with their defaults.

### Options

The `options` of `attachIframe` (`IbisOfficeOptions`). An item left out takes its default.

| Option | Type | Default | Meaning | Example |
|---|---|---|---|---|
| `locale` | `'ja'` \| `'en'` \| `'auto'` | `'auto'` (the browser's language) | The display language of the editor. | `locale: 'en'` |
| `theme` | `'light'` \| `'dark'` \| `'auto'` | `'auto'` (the OS's) | The colour scheme. | `theme: 'dark'` |
| `ui.menuBar` | boolean | `true` | Show the menu bar. | `ui: { menuBar: false }` |
| `ui.toolBar` | boolean | `true` | Show the toolbar. | `ui: { toolBar: false }` |
| `ui.closeButton` | boolean | `false` | Show the Close button at the top right. Pressed (or Esc with nothing unsaved, or ⌘W / Ctrl+W where the browser passes it to the page), it sends the `closeRequested` event after the question about unsaved changes. | `ui: { closeButton: true }` |
| `ui.downloadButton` | boolean | `false` | Show the Download button. Pressed, it sends the `download` event with the document as it is now. | `ui: { downloadButton: true }` |
| `ui.fontSetupPrompt` | boolean | `true` | Lead the user to the fonts that can be downloaded (the セットアップ dialog that opens by itself at the first start, and Help > セットアップ). | `ui: { fontSetupPrompt: false }` |
| `ui.startWindow` | boolean | `true` | `false`: no start view while there is no document (the window is blank until `load`), and no start window item in the menus. | `ui: { startWindow: false }` |
| `features.localFiles` | boolean | `true` | Show Open from the user's own computer and Save as a download. | `features: { localFiles: false }` |
| `features.print` | boolean | `true` | Printing. The bridge gives a default `printPdf` (the PDF opens in a new tab); `false` turns printing off. | `features: { print: false }` |
| `features.export` | boolean \| string[] | `true` (all formats) | Export (saved to your storage with `saveFileAs`). `false`: none. An array: only those formats. | `features: { export: ['pdf'] }` |
| `features.openLinks` | boolean | `true` | Opening links to the outside (through your `openUrl`). `false`: off. | `features: { openLinks: false }` |
| `features.fontDownload` | boolean | `true` | Downloading the extra fonts (the setup). Also off for the blocks of fonts. | `features: { fontDownload: false }` |
| `watermark` | `{ text, opacity?, includeInPrint? }` | none | A text drawn over the document. Not saved in the file. `text`: up to 100 characters; `opacity`: 0 to 1, default `0.12`; `includeInPrint`: default `true` (the printed PDF has it too). | `watermark: { text: 'Confidential', opacity: 0.1 }` |
| `user` | `{ id?, name }` | none | The user who works: `name` is the author of comments and tracked changes (up to 255 characters). `id` is accepted and not used by the editor. | `user: { name: 'Taro Yamada' }` |
| `sourceUrl` | string | the ibisOffice project's page | Where Help > source code (AGPL) goes: the source of the modified version you serve (AGPL section 13). `https:`, `http:` or `mailto:`. | `sourceUrl: 'https://example.com/ibisoffice-source'` |
| `startUrl` | string | none (the logo does nothing) | The address of your start page, a page that runs ibisOffice with no document. The logo at the left end of the menu bar opens it in a new tab through `openUrl`. `https:`, `http:` or `mailto:`. | `startUrl: 'https://example.com/office/'` |
| `resources.listUrl` | string | the public repository's `resource-list.txt` | The address of `resource-list.txt` (the sounds of slide transitions). `https:`, or `http://localhost` / `http://127.0.0.1`; a relative address is resolved against the page of the loader, and the lines' own `url` and `license_url` against the list's address. | `resources: { listUrl: '/ibisoffice/resources/resource-list.txt' }` |
| `resources.fontListUrl` | string | the public repository's `font-list.txt` | The address of `font-list.txt` (the fonts セットアップ offers, and the font the PDF falls back to). The same rules as `resources.listUrl`: `https:`, or `http://localhost` / `http://127.0.0.1`; a relative address is resolved against the page of the loader, and the lines' own `url` and `license_url` against the list's address. A value that is not an address the editor may fetch leaves the public list. | `resources: { fontListUrl: '/ibisoffice/fonts/font-list.txt' }` |
| `autoSaveSeconds` | number | none | In the type, but the editor does not read it yet: it makes no automatic save to the host (every save is the user's, or the close's). | |

The editor reads no other option. The playground's address parameters (`view`, `menuBar`, `toolBar`, `close`, `download`, `fd`, `origin`) are in [Turning the buttons on](#turning-the-buttons-on); an option given to `attachIframe` wins over the same one in the address.

### Documents: `handle.load`

`handle.load(doc)` opens a document and replaces the current one (`HostDocument`):

| Field | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | required | The file name with its extension (docx, xlsx, pptx). |
| `bytes` | ArrayBuffer | required | The file's contents. |
| `version` | string | required | The version of these contents. ibisOffice does not look inside; it hands it back as the next `baseVersion`. |
| `readOnly` | boolean | `false` | Open read-only. |
| `readOnlyMessage` | string | none | The text of the information bar when read-only (you decide the language). |
| `bound` | boolean | `true` | Whether the document is tied to your file. `false`: opened as a local document. |
| `folderId` | string | none | The id of the folder the file is in; the file window starts there. |

### Handle

What `attachIframe` returns (`IbisOfficeHandle`):

| Member | Meaning |
|---|---|
| `load(doc)` | Open a document (above). |
| `setReadOnly(readOnly, message?)` | Switch read-only on or off; `message` is the information bar's text. |
| `notify(notice \| null)` | Show a notice to the user in the information bar: `{ level: 'info' \| 'warn' \| 'error', message, id? }`. It replaces the previous one of the same level; `null` clears. |
| `snapshot()` | The current contents, `{ name, bytes }` (the same bytes as a save). For checking before you close. |
| `destroy()` | Clean up; call it before you remove the `iframe`. |

### Functions you give (`host`)

Each one is optional and returns a `Promise`. A failure is `{ ok: false, reason, message?, offerSaveAs? }`; `reason` is `locked`, `conflict`, `readOnly`, `noPermission`, `notFound`, `exists`, `tooLarge`, `unauthorized`, `network` or `other`; `message` is shown to the user (you decide the language).

| Function | When ibisOffice calls it | Receives | Returns on success |
|---|---|---|---|
| `listFolder` | The file window (Open, Save as, export) shows a folder. `folderId: null` is the list of top folders. | `{ folderId, purpose: 'open' \| 'saveAs' \| 'export', include: { folders, files }, extensions?, cursor?, limit }` (`limit` is 200) | `{ folder, folders, files, nextCursor?, deferredCursor? }`, one page |
| `checkSaveTarget` | Before Save as or export: whether the folder is writable and the name exists. Without it, ibisOffice reads the folder's remaining pages and compares names. | `{ folderId, name, purpose }` | `{ ok: true, canWrite, existing? }` |
| `openFile` | A file was chosen in the file window. You open it and call `load`. | `{ fileId }` | `{ ok: true }` |
| `saveFile` | Save (also auto: `trigger` is `'user'`, `'auto'` or `'close'`). | `{ bytes, baseVersion, trigger }` | `{ ok: true, version }` |
| `saveFileAs` | Save as, export, make a copy. For `'saveAs'` the document is tied to the new file; for `'export'` and `'copy'` it is not. | `{ kind: 'saveAs' \| 'export' \| 'copy', folderId, name, bytes, overwriteFileId? }` | `{ ok: true, file: { id, name }, version }` |
| `renameFile` | The file name's menu: rename (the extension does not change). | `{ name, baseVersion }` | `{ ok: true, name, version }` |
| `getFileLocation` | The file name's menu: get the path. ibisOffice only copies what you return to the clipboard. | none | `{ ok: true, url }` |
| `listRecentFiles` | File > Recent files (newest first, 20 at most). You keep the list wherever you like. | none | `{ ok: true, files: [{ name, url }] }` |
| `addRecentFile` | A file was opened or saved under a new name. Called only when you also gave `getFileLocation`; the same `url` is not listed twice. | `{ name, url }` | `{ ok: true }` |
| `openUrl` | A link in a document, or Help > source code. You open a new tab and check the address (only `https:`, `http:` and `mailto:` are passed). | `{ url }` | `{ ok: true }` |
| `printPdf` | Print. ibisOffice makes the PDF and hands it over. Without it the bridge opens the PDF in a new tab (off with `features.print: false`). | `{ name, bytes, tab }` (`tab`: the tab opened at the click, or `null`) | `{ ok: true }` |

The shapes of folders and files in `listFolder` (`HostFolder`, `HostFileEntry`, `rootKind`) are in `ibisoffice-host.d.ts`.

### Events (`onEvent`)

| `type` | When | Fields |
|---|---|---|
| `ready` | The editor started; `load` is accepted. | |
| `loaded` | A document was opened. | `name`, `kind` (`'docx'`, `'xlsx'` or `'pptx'`) |
| `dirty` | Whether there are unsaved changes changed. | `dirty` (boolean) |
| `documentChanged` | The document was replaced. | `source` (`'local'`, `'host'` or `'new'`) |
| `saved` | A save finished (your `saveFile` returned `ok`). | `trigger` (`'user'`, `'auto'` or `'close'`) |
| `closeRequested` | The Close button was pressed (after the question about unsaved changes). Close your view. | |
| `download` | The Download button was pressed. | `name`, `bytes` (the document as it is now, in its own format) |
| `error` | The editor could not start or load. | `message` |

## Turning the buttons on

The browser version (`www/`) can sit inside another page, for example a cloud service's file preview. Two optional buttons at the top right of the window let the page that embeds it handle closing and downloading itself. **Both are off by default**, so the playground looks the same unless you ask.

- **Close**: the button at the far right.
- **Download** (icon): the button to its left.

Pressing a button does not close or save anything itself. The editor tells the page that embeds it, and the page does what it wants (close its dialog, save the file to its own storage, and so on).

From the URL of the editor:

| Parameter | Meaning |
|---|---|
| `view=1` | Open in read-only viewing mode (`0` or `false` turns it off). The menu bar and toolbar remain shown by default even when `view=1`. |
| `menuBar=0` | Hide the menu bar (shown by default, `menuBar=1`). |
| `toolBar=0` | Hide the toolbar (shown by default, `toolBar=1`). |
| `fd=0` | Do not lead the user to the fonts that can be downloaded (the セットアップ dialog that opens by itself at the first start of a version, and Help > セットアップ). On by default (`fd=1`). The playground starts with `fd=0`. |
| `close=1` | Show the Close button (`0` or `false` turns it off). |
| `download=1` | Show the Download button. |
| `origin=https://app.example.com` | The origin that messages to the parent window are sent to (iframe only; see below). |

From JavaScript, in a script that runs **before** `main.js`:

```html
<script>
  window.ibisOfficeOptions = {
    view: true,
    menuBar: false,
    toolBar: false,
    close: true,
    download: true,
    fd: false, // no guidance to the font downloads
    targetOrigin: 'https://app.example.com', // iframe only
    eventTarget: document.getElementById('preview'), // optional; default: window
  };
</script>
<script type="module" src="main.js"></script>
```

A value given from JavaScript wins over the same one in the URL.

## What the page receives

### Same page (not in an iframe, or in addition to the iframe messages)

A `CustomEvent` is dispatched on `window` (or on `eventTarget`):

```js
window.addEventListener('ibisoffice:close', () => { /* close the preview */ });
window.addEventListener('ibisoffice:download', (e) => {
  const { name, bytes, blob } = e.detail; // name: file name, bytes: Uint8Array, blob: Blob
});
```

`download` carries the document **as it is now** (with the edits), in its own format (the same bytes as File > Save would write).

### In an iframe

The parent window also receives a message:

```js
window.addEventListener('message', (e) => {
  if (e.origin !== 'https://ibisoffice.example.com') return; // the editor's own origin
  const m = e.data;
  if (m?.source !== 'ibisoffice') return;
  if (m.type === 'close') { /* … */ }
  if (m.type === 'download') { /* m.name: string, m.bytes: ArrayBuffer */ }
});
```

The message is posted with the target origin from `origin=` / `targetOrigin`. Without it, the origin of `document.referrer` (the embedding page) is used. If neither is known, **nothing is posted** (a warning is logged), so a file is never sent to an unknown page. Pass `origin=*` (or `targetOrigin: '*'`) only if you really want any page to receive it.

## Letting the page keep the files (the host interface)

A page that embeds the browser version can also be the place where the files live: a cloud storage's viewer, for instance. The page (the
"host") hands ibisOffice a few functions; ibisOffice calls only those. It does not know the host's API, URLs or accounts; it hands over bytes,
names and versions, and the host decides how to store them (its locks, its permissions, its sign-in). A host that gives no function leaves the
editor as it was (the playground).

Start the page with `?host=1` (the smallest page, the options, the functions and the events are listed [above](#minimal-setup)). The files of the Web archive for this are `ibisoffice-host.js` (the bridge, run on the host's page) and
`ibisoffice-host.d.ts` (the types). Both ship in `ibisOffice-<version>-Web.zip`; use the ones of the version you serve.

In the host's repository, take `ibisoffice-host.js` and `ibisoffice-host.d.ts` out of the archive of the version you serve and keep them under the
same names in one folder (for example `src/office/vendor/`). Do not edit them; when you move to another version, replace both with that version's
(the `.js` starts with the AGPL-3.0-only copyright notice, and the `.d.ts` has the same one). When `attachIframe` cannot connect it throws an
`IbisOfficeConnectError` (exported by `ibisoffice-host.js`); tell the cause by its `reason` (`originMismatch`, `loaderTimeout`, `helloTimeout`,
`connectTimeout`, `protocolMismatch`, `refused`), not by the message.

- **Same origin as the host** (the page serves ibisOffice itself): the functions are handed over as they are and called directly. Nothing is
  posted. Give the iframe no `sandbox` attribute (a same-origin frame with scripts could take it off), and serve it under a Content-Security-Policy
  of `script-src 'self' 'wasm-unsafe-eval'` (no `unsafe-eval`; ibisOffice runs under that) and `frame-ancestors 'self'`.
- **Another origin** (`attachIframe(iframe, { officeOrigin: 'https://office.example.com', ... })`, with `?host=1&origin=<the host's origin>`):
  after a `hello` and a `connect`, the two sides talk through one `MessageChannel` port. The bytes are transferred.
- Each function is optional (see the table [above](#functions-you-give-host)). `listFolder` gives the file window (開く, 名前を付けて保存, 書き出し; the folders are fetched when a node is opened,
  1000 at a time). `saveFile` makes 保存 work; without a bound document it asks where. `renameFile` and `getFileLocation` fill the menu at the file
  name. `openUrl` opens the links. `printPdf` is given by the bridge unless `features.print` is false. A failure is `{ ok: false, reason, message? }`;
  the reasons are `locked`, `conflict`, `readOnly`, `noPermission`, `notFound`, `exists`, `tooLarge`, `unauthorized`, `network`, `other`.
- The Web version's ファイル > コピーの保存 downloads the copy at once.
- ibisOffice never opens a link itself: a link in a document goes to `openUrl` (only `https:`, `http:` and `mailto:` are passed, and the host
  checks again). It reads and writes no cookie or `sessionStorage` of the page and, of `localStorage`, only the file window's size and its splitter's place
  (`ibisOffice.dialog.host-file.*`); its other settings are kept in memory while a host is there, and registers no Service Worker. `scripts/check-web-sources.py` checks this in the sources and in the built files.
- `?view=1`, `?menuBar=0`, `?toolBar=0`, `?close=1`, `?download=1`, `?fd=0`, `?origin=` and the events above stay as they were. Options given
  to `attachIframe` win over the same ones of the address.

## Pinning a version

The files of a release do not change once it is published, so a version you have checked (see [Get the browser version](#get-the-browser-version)) stays the same.

- Keep the archive's SHA-256 in your own repository and compare it again each time you fetch.
- To check the files you actually serve, run the last command of the check inside the folder you deploy.
- `license.md` (the third-party licences of the browser version) and `assets/artborders/ECMA-NOTICE.txt` are in the archive. Keep them when you serve the files.
- The `.wasm` file is large; a server that compresses it makes the download much smaller.
- The desktop apps do not use these files (their automatic update has its own).

## Serving the wasm in parts (for a CDN that limits the size of a file)

Some CDNs refuse a file above a limit (Cloudflare Workers Static Assets: 25 MiB), and the `.wasm` file is larger. `scripts/web-loader-setup.mjs` (Node.js 18 or later; nothing to install) cuts it into parts and writes `loader-manifest.json`; the page then fetches the parts in parallel, checks each one (size and SHA-256) before using it, and starts the app while they arrive. Without the manifest the page is as before (one file). The release archive is not cut: whoever serves it runs the tool, with the limit of their own CDN.

```sh
node scripts/web-loader-setup.mjs app --chunk 20 --out app-parts      # parts of 20 MiB or less, packed with gzip
```

- `--chunk <MiB>` is the size of a stored part; `--encoding gzip` (the default) packs the file before it is cut and the browser expands it, so the download is about a quarter of the raw size even when the CDN does not compress `.part` files; `--encoding none` cuts the file as it is (then let the CDN compress the parts). `--also <path>` cuts other files too; `--out` puts the parts and the manifest in another folder (without it, beside the files; the originals are never changed, so the SHA-256 list of the archive still holds). The tool joins and expands what it wrote and compares it with the file before it ends.
- Serve the parts and `loader-manifest.json` from the same directory as `index.html` (the parts as `pkg/*.part`). The parts have the hash of the file in their names (cache them for a long time); `loader-manifest.json` is better revalidated.
- The manifest is the truth. `?chunk=<MiB>` or `ibisOfficeOptions.chunk` only says that the page is served in parts: when it differs from the manifest the manifest is used and the console says so; a manifest of another version than `main.js` stops the start (a mixed deploy).
- A part that fails its check is fetched again (3 times, with a wait); then the start stops and the status line names the part. Where `crypto.subtle` is missing (a page that is not a secure context, other than `localhost`) the parts are checked by size only, and the console warns.

### Fonts in blocks (the browser version takes only what a document needs)

The fonts of セットアップ that are under the SIL Open Font License (BIZ UD Gothic, BIZ UD Mincho, ACGJMincho) can be served in blocks instead of whole. `tools/ibis-font-subsetter` cuts a font into a base block (ASCII and the repertoire of JIS X 0208, about 3 MB) and blocks of the rest of its characters in the order of their code points (512 KiB or less each after deflating; `--chunk` changes it), each block a font of its own named `ibis <id> <key>` (the name of the font it was cut from is not carried: the copyright and the licence are kept). `web-loader-setup.mjs --subset <folder>` puts the blocks, each font's licence and README beside the page and lists them in `loader-manifest.json`. The page takes the base block of each font when it starts and the other blocks a document needs; a block is asked for by its name (not by the characters of the document), checked (size and SHA-256), and kept in the browser (IndexedDB). The viewer's own fonts draw what a block has not come for. A host that turns `features.fontDownload` off gets no blocks. IPAmj Mincho is not cut (its licence does not allow it): it stays a download of セットアップ.

```sh
cargo run --release -p ibis-font-subsetter -- --font BIZUDGothic-Regular.ttf --id BIZUDGothic --typeface "BIZ UDゴシック" --typeface "BIZ UDGothic" --license LICENSE --out blocks
node scripts/web-loader-setup.mjs app --subset blocks --out app-parts
```

## Licence

ibisOffice is under AGPL-3.0-only and a commercial licence. If you serve a modified version to users over a network, AGPL section 13 asks you to offer them its source: give the address in `sourceUrl`. To use ibisOffice without AGPL's conditions, see [COMMERCIAL-LICENSE.md](../COMMERCIAL-LICENSE.md).

## Notes

- The desktop apps are not affected.
- The button labels follow the editor's language (English or Japanese).
