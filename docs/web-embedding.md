# Embedding the browser version

**English** | [日本語](web-embedding-ja.md)

The browser version (`www/`) can sit inside another page, for example a cloud service's file preview. Two optional buttons at the top right of the window let the page that embeds it handle closing and downloading itself. **Both are off by default**, so the playground looks the same unless you ask.

- **Close**: the button at the far right.
- **Download** (icon): the button to its left.

Pressing a button does not close or save anything itself. The editor tells the page that embeds it, and the page does what it wants (close its dialog, save the file to its own storage, and so on).

## Turning the buttons on

From the URL of the editor:

| Parameter | Meaning |
|---|---|
| `view=1` | Open in read-only viewing mode (`0` or `false` turns it off). The menu bar and toolbar remain shown by default even when `view=1`. |
| `menuBar=0` | Hide the menu bar (shown by default, `menuBar=1`). |
| `toolBar=0` | Hide the toolbar (shown by default, `toolBar=1`). |
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

## Fetching a fixed version and checking it

Every release attaches the browser version as an archive, so you can pin the version you serve and check that what you fetched is intact. The files are the same as the playground's `app/` folder (`index.html`, `main.js`, `license.md`, `pkg/`, `assets/`), starting at the top of the archive:

| File (`<version>` is e.g. `0.8.5`) | What it is |
|---|---|
| `ibisOffice-<version>-Web.zip` | The browser version |
| `ibisOffice-<version>-Web.zip.sha256` | SHA-256 of the archive |
| `ibisOffice-<version>-Web.files.sha256` | SHA-256 of each file inside the archive, by path |

All three are on the release page: `https://github.com/ibisjp/ibis-office/releases/tag/v<version>`. Fetch them, check the archive, unzip it, and check each file:

```sh
V=0.8.5
BASE=https://github.com/ibisjp/ibis-office/releases/download/v$V
curl -LO $BASE/ibisOffice-$V-Web.zip
curl -LO $BASE/ibisOffice-$V-Web.zip.sha256
curl -LO $BASE/ibisOffice-$V-Web.files.sha256

shasum -a 256 -c ibisOffice-$V-Web.zip.sha256        # macOS (Linux: sha256sum -c ...)
mkdir app && unzip -q ibisOffice-$V-Web.zip -d app
(cd app && shasum -a 256 -c ../ibisOffice-$V-Web.files.sha256)   # every line must say OK
```

On Linux, `sha256sum -c` takes the same files; on Windows PowerShell, compare `(Get-FileHash ibisOffice-$V-Web.zip -Algorithm SHA256).Hash` with the hash in the `.sha256` file (case-insensitive). To check the files you actually serve, run the last command inside the folder you deploy.

Notes:

- The release files do not change once published, so a version you have checked stays the same. Keep the archive's SHA-256 in your own repository and compare it again when you fetch.
- `license.md` (the third-party licences of the browser version) and `assets/artborders/ECMA-NOTICE.txt` are in the archive. Keep them when you serve the files.
- Serve the folder as it is. The `.wasm` file is large (about 65 MB); a server that compresses it makes the download much smaller.
- The desktop apps do not use these files (their automatic update has its own).

## Notes

- The desktop apps are not affected.
- The button labels follow the editor's language (English or Japanese).
