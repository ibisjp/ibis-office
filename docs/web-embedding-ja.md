# ブラウザー版の組み込み

[English](web-embedding.md) | **日本語**

ibisOffice のブラウザー版は、静的なファイルの集まり（ローダー・WebAssembly 本体・いくつかの素材）です。自分の Web サーバーに置き、ご自身のページの `iframe` で読み込み、ページ自身の関数（ファイルの一覧・開く・保存）を ibisOffice に渡します。ibisOffice はブラウザーの中で Word（.docx）・Excel（.xlsx）・PowerPoint（.pptx）の文書を描いて編集し、ファイル・ログイン・権限はご自身のシステムが持ちます。エディターは閲覧者のブラウザーの中で動き、渡された関数だけを呼びます。

このページの順序：

1. [ブラウザー版を入手する](#ブラウザー版を入手する)：アーカイブのダウンロードと確認。
2. [最小限の設置](#最小限の設置)：ファイルの置き場所、サーバーに必要な設定、文書を開く最小のページ。
3. [オプションとホストのインターフェースの一覧](#オプションとホストのインターフェースの一覧)：すべてのオプション・関数・イベント。
4. [ボタンを出す](#ボタンを出す)・[ページが受け取るもの](#ページが受け取るもの)・[ホストのインターフェース](#ページがファイルを預かるホストのインターフェース)：「閉じる」「ダウンロード」のボタン、イベント、ホストの詳しい話。
5. [版を固定する](#版を固定する)と、[wasm を分割して配る](#wasm-を分割して配るファイルの大きさに制限のある-cdn-向け)話。

## ブラウザー版を入手する

いちばん手軽なのは `npm install @ibisjp/ibis-office` です（[npm から入れる](#npm-から入れる)）。同じファイルを、各リリースにアーカイブとしても付けています。

リリースごとに、ブラウザー版をアーカイブにして添付しています。[v0.8.9](https://github.com/ibisjp/ibis-office/releases/tag/v0.8.9) のブラウザー版は、次の 3 つです。

| ファイル | 内容 |
|---|---|
| [`ibisOffice-0.8.9-Web.zip`](https://github.com/ibisjp/ibis-office/releases/download/v0.8.9/ibisOffice-0.8.9-Web.zip) | ブラウザー版 |
| [`ibisOffice-0.8.9-Web.zip.sha256`](https://github.com/ibisjp/ibis-office/releases/download/v0.8.9/ibisOffice-0.8.9-Web.zip.sha256) | アーカイブの SHA-256 |
| [`ibisOffice-0.8.9-Web.files.sha256`](https://github.com/ibisjp/ibis-office/releases/download/v0.8.9/ibisOffice-0.8.9-Web.files.sha256) | アーカイブの中の各ファイルのパスと SHA-256 |

各版のファイルは `https://github.com/ibisjp/ibis-office/releases/download/v<版>/<ファイル名>` にあります。公開したリリースのファイルは、後から変わりません。

取ってきたものを確かめます。

```sh
V=<版>
BASE=https://github.com/ibisjp/ibis-office/releases/download/v$V
curl -LO $BASE/ibisOffice-$V-Web.zip
curl -LO $BASE/ibisOffice-$V-Web.zip.sha256
curl -LO $BASE/ibisOffice-$V-Web.files.sha256

shasum -a 256 -c ibisOffice-$V-Web.zip.sha256        # macOS（Linux は sha256sum -c ...）
mkdir app && unzip -q ibisOffice-$V-Web.zip -d app
(cd app && shasum -a 256 -c ../ibisOffice-$V-Web.files.sha256)   # すべての行が OK になること
```

Windows の PowerShell では、`(Get-FileHash ibisOffice-$V-Web.zip -Algorithm SHA256).Hash` の値を `.sha256` ファイルのハッシュと比べます（大文字小文字は区別しません）。

アーカイブには、次の節でページから読み込む `ibisoffice-host.js` が入っている必要があります。`unzip -l ibisOffice-$V-Web.zip` で確かめてください。ホストのインターフェースより前の古い版のアーカイブには入っていません。

## npm から入れる

同じファイルを npm の [`@ibisjp/ibis-office`](https://www.npmjs.com/package/@ibisjp/ibis-office) でも配っています。パッケージの版はリリースの版と同じです（`0.9.0` は v0.9.0 のブラウザー版）。ホストのローダー（ページが import するもの）と、エディターのファイル（`editor/`）が入っています。

```sh
npm install @ibisjp/ibis-office          # package.json に 1 行でも可："@ibisjp/ibis-office": "^0.9.0"
npx ibis-office-copy public/ibisoffice   # エディターを public/ibisoffice/<版>/ に写す
```

写したフォルダーは「[最小限の設置](#最小限の設置)」のとおりに配信します。ページは `attachIframe` を `@ibisjp/ibis-office` から（バンドラーか import map で）、または写したフォルダーの `ibisoffice-host.js` から import します。新しいリリースを使うときは `npm update @ibisjp/ibis-office` のあと、もう一度 `npx ibis-office-copy` を実行します。版ごとのフォルダーにすると、長くキャッシュできます。`--flat` を付けると、指定したフォルダーにそのまま写します。パッケージはインストール時のスクリプトを持たず、`ibis-office-copy` は Node の標準だけを使います。ライセンスはアーカイブと同じです（「ライセンス」の節）。

## 最小限の設置

### アーカイブの中身

アーカイブは、そのまま配るフォルダーです（直下から並んでいます）。

| ファイル | 内容 |
|---|---|
| `index.html` | エディターを動かすページ。ご自身のページの `iframe` が `?host=1` を付けて指します。インラインのスクリプトはありません。 |
| `boot.js`・`main.js`・`host-loader.js`・`chunked-fetch.js`・`subset-fetch.js`・`build-info.js` | ローダー。`index.html` と同じディレクトリに置きます。 |
| `pkg/` | WebAssembly 本体（`ibis_office_app_bg.wasm`。数十 MB）、そのグルー（`ibis_office_app.js`）、`ibis_office_app_bg.wasm.size`（ダウンロードの進み具合の分母になる大きさ）。 |
| `assets/` | アイコンと、ページ罫線の絵柄（`artborders/ECMA-NOTICE.txt` を含む）。 |
| `ibisoffice-host.js`・`ibisoffice-host.d.ts` | ご自身のページが読み込むもの。`attachIframe` と、オプション・関数・イベントの型。 |
| `license.md` | ブラウザー版の第三者ライセンス表示。 |

### 置く場所

展開したフォルダーを、Web サーバーが静的ファイルとして配る場所に置き、**中のファイルは動かさずそのまま配ってください**。版ごとにフォルダーを分けるのがいちばん簡単です。

```text
/ibisoffice/0.8.9/index.html
/ibisoffice/0.8.9/main.js
/ibisoffice/0.8.9/ibisoffice-host.js
/ibisoffice/0.8.9/pkg/ibis_office_app_bg.wasm
...
```

配るときは、`license.md` と `assets/artborders/ECMA-NOTICE.txt` も一緒に置いてください。

### サーバーに必要な設定

| 項目 | すること |
|---|---|
| HTTPS | ご自身のページもエディターも HTTPS で配ります（開発中は `http://localhost` でも動きます）。エディターは WebGPU で描き、ブラウザーは WebGPU を安全なページにだけ出します。対応するのは Chrome・Edge・Safari 26 以降です。 |
| 同じオリジン | エディターは、ご自身のページと同じオリジンから配るのが基本です（このページが示す形）。別のオリジンでも、メッセージを介して動きます。[ホストのインターフェース](#ページがファイルを預かるホストのインターフェース)の「別のオリジン」を見てください。 |
| `.wasm` の型 | `.wasm` は `application/wasm`、`.js` は `text/javascript` で返します。 |
| 圧縮 | `.wasm` と `.js` は圧縮（gzip か Brotli）して返します。wasm は数十 MB で、圧縮すると約 4 分の 1 になります。 |
| キャッシュ | ファイル名にハッシュは付いていません。版ごとのフォルダー（上）に置くなら長くキャッシュし（`Cache-Control: public, max-age=31536000, immutable`）、次の版は別のフォルダーに置きます。固定のフォルダーに置くなら、`Cache-Control: no-cache` と `ETag` で、新しい版が使われるようにします。 |
| `COOP`・`COEP` ヘッダー | 要りません。エディターは 1 スレッドで動き、`SharedArrayBuffer` を使いません。 |
| Content-Security-Policy | 付けるなら、`script-src 'self' 'wasm-unsafe-eval'`（`unsafe-eval` は不要）と、ページには `frame-ancestors 'self'` で足ります。`index.html` に `style` 要素が 1 つあるので、`style-src` を設けるときは許可してください。次の節の外部のアドレスを使うなら、`connect-src` にも許可を足します。 |
| `iframe` | `sandbox` 属性は付けません。 |

### エディターが外から取るもの

アーカイブに入っていないものが 2 種類あります。文書が必要としたときだけ取ります。

- **画面切り替えの音**（`resource-list.txt` と、そこに書かれた音）。既定では、`https://raw.githubusercontent.com/ibisjp/ibis-office/main/docs/resource-list.txt` と、`https://ibisjp.github.io/ibis-office/sounds/` の音を読みます。自分のサーバーで配るには、`docs/resource-list.txt` と `sounds/<id>/` のフォルダー（それぞれの `LICENSE` を含む）を自分のサーバーに写し、各行の `url` と `license_url` を、一覧と同じディレクトリを基準にした相対パス（例：`sounds/applause/applause.ogg`）に直し、一覧のアドレスをオプションの `resources.listUrl` に渡します（相対アドレスは、ローダーのページを基準に解決します）。取ったあと、行の `size`（と、あれば `sha256`）で確かめます。
- **書体**。セットアップがダウンロードを案内する書体（と、そこへ導く案内）です。既定の一覧は `https://raw.githubusercontent.com/ibisjp/ibis-office/main/docs/font-list.txt` です。自分のサーバーで配るには、`docs/font-list.txt` と、そこに書かれた書体のファイル（それぞれのフォルダーにライセンスのファイルを添えて）を自分のサーバーに写し、各行の `url` と `license_url` を一覧と同じディレクトリを基準にした相対パス（例：`fonts/ACGJMincho/acgjm.ttf`）に直し、一覧のアドレスを `resources.fontListUrl` に渡します。配る前に、各書体のライセンスを確かめてください（フォルダーにあります）。取ったあと、行の `size`（と、あれば `sha256`）で確かめます。案内は `ui.fontSetupPrompt: false`、ダウンロードは `features.fontDownload: false` で止めます。SIL Open Font License の書体をチャンクで配るには、[書体をチャンクで配る](#書体をチャンクで配るブラウザー版は文書に要るものだけ取る)をご覧ください。`features.fontDownload: false` のページは、チャンクも取りません。

音や書体が取れなくても、エディターはそれなしで動き続けます。

### 文書を開く最小のページ

ご自身のページがエディターを `iframe` に入れ、`attachIframe` でつなぎ、`load` に文書を渡します。次の例では、ページのサーバーがファイルを `/api/files/42`（例です。どんな保存先でもかまいません）に持ち、エディターは `/ibisoffice/0.8.9/` から配っています。

```html
<!doctype html>
<meta charset="utf-8">
<iframe id="office" src="/ibisoffice/0.8.9/index.html?host=1"
        style="width:100%; height:100vh; border:0"></iframe>
<script type="module">
  import { attachIframe } from '/ibisoffice/0.8.9/ibisoffice-host.js';

  const handle = await attachIframe(document.getElementById('office'), {
    options: { locale: 'ja', user: { name: '山田太郎' }, ui: { closeButton: true, startWindow: false } },
    host: {
      // 「保存」で呼ばれます。新しい版か、失敗の理由を返します。
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

動き：`attachIframe` が `iframe` の中のローダーを待ち、オプションと関数を渡します。`load` が文書を開き、利用者が編集し、「保存」で `saveFile` が、文書自身の形式のバイト列（ファイル > 保存が書くものと同じ）を受けて呼ばれます。ibisOffice は `saveFile` の先の URL もログインも知りません。渡された `version` を、次の保存の `baseVersion` として返すだけです。

渡さない関数は、提供しない機能になります。`listFolder` が無ければファイル選択ウィンドウは出ず（ローカルのファイル選択とダウンロードを使います）、`saveFile` が無ければ「保存」は保存先を尋ねます。`attachIframe` が接続できなかったときは `IbisOfficeConnectError` を投げ、その `reason` で原因が分かります（[ホストのインターフェース](#ページがファイルを預かるホストのインターフェース)）。

## オプションとホストのインターフェースの一覧

型は `ibisoffice-host.d.ts` にあります。ここでは既定値と一緒に並べます。

### オプション

`attachIframe` の `options`（`IbisOfficeOptions`）です。書かなかった項目は既定値になります。

| オプション | 型 | 既定値 | 意味 | 例 |
|---|---|---|---|---|
| `locale` | `'ja'` \| `'en'` \| `'auto'` | `'auto'`（ブラウザーの言語） | エディターの表示言語。 | `locale: 'ja'` |
| `theme` | `'light'` \| `'dark'` \| `'auto'` | `'auto'`（OS の設定） | 配色。 | `theme: 'dark'` |
| `ui.menuBar` | boolean | `true` | メニューバーを出す。 | `ui: { menuBar: false }` |
| `ui.toolBar` | boolean | `true` | ツールバーを出す。 | `ui: { toolBar: false }` |
| `ui.closeButton` | boolean | `false` | 右上に「閉じる」ボタンを出す。押すと（保存していない変更があれば、その確認のあとに）`closeRequested` イベントを送ります。変更が無いときの Esc と、ブラウザーがページに渡す環境での ⌘W / Ctrl+W も同じです。 | `ui: { closeButton: true }` |
| `ui.downloadButton` | boolean | `false` | 「ダウンロード」ボタンを出す。押すと、今の文書を載せた `download` イベントを送ります。 | `ui: { downloadButton: true }` |
| `ui.fontSetupPrompt` | boolean | `true` | ダウンロードできる書体への誘導（初回起動で自動で開くセットアップのダイアログと、ヘルプ > セットアップ）を出す。 | `ui: { fontSetupPrompt: false }` |
| `ui.startWindow` | boolean | `true` | `false`：文書が無いあいだ、スタート画面を出さない（`load` まで窓は空白）。メニューにもスタート画面の項目を出さない。 | `ui: { startWindow: false }` |
| `features.localFiles` | boolean | `true` | 利用者自身のコンピューターからの「開く」と、ダウンロードとしての「保存」を出す。 | `features: { localFiles: false }` |
| `features.print` | boolean | `true` | 印刷。ブリッジが既定の `printPdf`（PDF を新しいタブで開く）を渡します。`false` で印刷を止めます。 | `features: { print: false }` |
| `features.export` | boolean \| string[] | `true`（すべての形式） | 書き出し（`saveFileAs` で自分の保存先に保存）。`false`：なし。配列：その形式だけ。 | `features: { export: ['pdf'] }` |
| `features.openLinks` | boolean | `true` | 外へのリンクを開く（`openUrl` を介す）。`false` で止めます。 | `features: { openLinks: false }` |
| `features.fontDownload` | boolean | `true` | 追加の書体のダウンロード（セットアップ）。書体のチャンクも止まります。 | `features: { fontDownload: false }` |
| `watermark` | `{ text, opacity?, includeInPrint? }` | なし | 文書の上に重ねて描く文字。ファイルには保存しません。`text`：100 文字まで。`opacity`：0〜1、既定 `0.12`。`includeInPrint`：既定 `true`（印刷の PDF にも入れる）。 | `watermark: { text: '社外秘', opacity: 0.1 }` |
| `user` | `{ id?, name }` | なし | 作業する利用者。`name` はコメントと変更履歴の作成者になります（255 文字まで）。`id` は受け付けますが、エディターは使いません。 | `user: { name: '山田太郎' }` |
| `sourceUrl` | string | ibisOffice プロジェクトのページ | ヘルプ > ソースコード（AGPL）の行き先。配信している改変版のソース（AGPL 第 13 条）。`https:`・`http:`・`mailto:`。 | `sourceUrl: 'https://example.com/ibisoffice-source'` |
| `startUrl` | string | なし（ロゴは何もしない） | ご自身のスタート ページ（文書なしで ibisOffice を動かすページ）のアドレス。メニューバー左端のロゴが、`openUrl` を介して新しいタブで開きます。`https:`・`http:`・`mailto:`。 | `startUrl: 'https://example.com/office/'` |
| `resources.listUrl` | string | 公開リポジトリの `resource-list.txt` | `resource-list.txt`（画面切り替えの音）のアドレス。`https:`、または `http://localhost`・`http://127.0.0.1`。相対アドレスはローダーのページを基準に、一覧の各行の `url`・`license_url` は一覧のアドレスを基準に解決します。 | `resources: { listUrl: '/ibisoffice/resources/resource-list.txt' }` |
| `resources.fontListUrl` | string | 公開リポジトリの `font-list.txt` | `font-list.txt`（セットアップが案内する書体と、PDF が代わりに使う書体）のアドレス。規則は `resources.listUrl` と同じで、`https:`、または `http://localhost`・`http://127.0.0.1`。相対アドレスはローダーのページを基準に、一覧の各行の `url`・`license_url` は一覧のアドレスを基準に解決します。エディターが取ってよいアドレスでない値は無視し、公開の一覧を使います。 | `resources: { fontListUrl: '/ibisoffice/fonts/font-list.txt' }` |
| `autoSaveSeconds` | number | なし | 型にはありますが、エディターはまだ読みません。ホストへの自動保存はしません（保存は利用者の操作か、閉じるときだけ）。 | |

エディターが読むオプションは以上です。playground のアドレスのパラメーター（`view`・`menuBar`・`toolBar`・`close`・`download`・`fd`・`origin`）は[ボタンを出す](#ボタンを出す)にあります。`attachIframe` に渡したオプションが、アドレスの同じ項目より優先されます。

### 文書：`handle.load`

`handle.load(doc)` は、今の文書を置き換えて文書を開きます（`HostDocument`）。

| 項目 | 型 | 既定値 | 意味 |
|---|---|---|---|
| `name` | string | 必須 | 拡張子つきのファイル名（docx・xlsx・pptx）。 |
| `bytes` | ArrayBuffer | 必須 | ファイルの中身。 |
| `version` | string | 必須 | この中身の版。ibisOffice は中を見ず、次の `baseVersion` としてそのまま返します。 |
| `readOnly` | boolean | `false` | 読み取り専用で開く。 |
| `readOnlyMessage` | string | なし | 読み取り専用のときの情報バーの文（言語はホストが決めます）。 |
| `bound` | boolean | `true` | 文書をホストのファイルに結びつけるか。`false`：ローカルの文書として開く。 |
| `folderId` | string | なし | ファイルがあるフォルダーの id。ファイル選択ウィンドウがそこから始まります。 |

### ハンドル

`attachIframe` が返すもの（`IbisOfficeHandle`）です。

| メンバー | 意味 |
|---|---|
| `load(doc)` | 文書を開く（上）。 |
| `setReadOnly(readOnly, message?)` | 読み取り専用を切り替える。`message` は情報バーの文。 |
| `notify(notice \| null)` | 利用者への知らせを情報バーに出す：`{ level: 'info' \| 'warn' \| 'error', message, id? }`。同じ level の前の知らせを置き換えます。`null` で消す。 |
| `snapshot()` | 今の中身 `{ name, bytes }`（保存と同じバイト列）。閉じる前の確認用。 |
| `destroy()` | 後片付け。`iframe` を取り除く前に呼びます。 |

### ホストが渡す関数（`host`）

どれも省略でき、`Promise` を返します。失敗は `{ ok: false, reason, message?, offerSaveAs? }` で、`reason` は `locked`・`conflict`・`readOnly`・`noPermission`・`notFound`・`exists`・`tooLarge`・`unauthorized`・`network`・`other`、`message` は利用者に見せる文です（言語はホストが決めます）。

| 関数 | 呼ばれるとき | 受け取るもの | 成功のときの戻り値 |
|---|---|---|---|
| `listFolder` | ファイル選択ウィンドウ（開く・名前を付けて保存・書き出し）がフォルダーを表示するとき。`folderId: null` は最上位のフォルダーの一覧。 | `{ folderId, purpose: 'open' \| 'saveAs' \| 'export', include: { folders, files }, extensions?, cursor?, limit }`（`limit` は 200） | `{ folder, folders, files, nextCursor?, deferredCursor? }`（1 ページ） |
| `checkSaveTarget` | 名前を付けて保存・書き出しの前に、フォルダーに書けるか、同じ名前があるかを尋ねる。無ければ、フォルダーの残りのページを読んで名前を比べます。 | `{ folderId, name, purpose }` | `{ ok: true, canWrite, existing? }` |
| `openFile` | ファイル選択ウィンドウでファイルが選ばれたとき。ホストが開き、`load` を呼びます。 | `{ fileId }` | `{ ok: true }` |
| `saveFile` | 保存（自動のときもあります。`trigger` は `'user'`・`'auto'`・`'close'`）。 | `{ bytes, baseVersion, trigger }` | `{ ok: true, version }` |
| `saveFileAs` | 名前を付けて保存・書き出し・コピーの保存。`'saveAs'` は文書が新しいファイルに結びつき、`'export'`・`'copy'` は結びつきません。 | `{ kind: 'saveAs' \| 'export' \| 'copy', folderId, name, bytes, overwriteFileId? }` | `{ ok: true, file: { id, name }, version }` |
| `renameFile` | ファイル名のメニューの「名前の変更」（拡張子は変わりません）。 | `{ name, baseVersion }` | `{ ok: true, name, version }` |
| `getFileLocation` | ファイル名のメニューの「パスを取得」。返したものを、ibisOffice はクリップボードに写すだけです。 | なし | `{ ok: true, url }` |
| `listRecentFiles` | ファイル > 最近使ったファイル（新しい順、20 件まで）。一覧はホストが好きな場所に持ちます。 | なし | `{ ok: true, files: [{ name, url }] }` |
| `addRecentFile` | ファイルを開いた、または別の名前で保存したとき。`getFileLocation` も渡したときだけ呼ばれ、同じ `url` は 2 度並びません。 | `{ name, url }` | `{ ok: true }` |
| `openUrl` | 文書の中のリンクや、ヘルプ > ソースコードのとき。ホストが新しいタブで開き、アドレスを確かめます（渡すのは `https:`・`http:`・`mailto:` だけ）。 | `{ url }` | `{ ok: true }` |
| `printPdf` | 印刷。ibisOffice が PDF を作って渡します。無ければ、ブリッジが PDF を新しいタブで開きます（`features.print: false` で止める）。 | `{ name, bytes, tab }`（`tab`：クリックで開いたタブ、または `null`） | `{ ok: true }` |

`listFolder` が扱うフォルダーとファイルの形（`HostFolder`・`HostFileEntry`・`rootKind`）は `ibisoffice-host.d.ts` にあります。

### イベント（`onEvent`）

| `type` | いつ | 項目 |
|---|---|---|
| `ready` | エディターが起動した（`load` を受け付ける）。 | |
| `loaded` | 文書を開いた。 | `name`、`kind`（`'docx'`・`'xlsx'`・`'pptx'`） |
| `dirty` | 未保存の変更があるかどうかが変わった。 | `dirty`（boolean） |
| `documentChanged` | 文書が入れ替わった。 | `source`（`'local'`・`'host'`・`'new'`） |
| `saved` | 保存が終わった（`saveFile` が `ok` を返した）。 | `trigger`（`'user'`・`'auto'`・`'close'`） |
| `closeRequested` | 「閉じる」が押された（未保存の確認のあと）。ホストの表示を閉じてください。 | |
| `download` | 「ダウンロード」が押された。 | `name`、`bytes`（今の文書。文書自身の形式） |
| `error` | 起動や読み込みに失敗した。 | `message` |

## ボタンを出す

ブラウザー版（`www/`）は、クラウドサービスのファイルのプレビューなど、ほかのページの中に置けます。窓の右上に 2 つのボタンを出せば、組み込んだ側のページが「閉じる」と「ダウンロード」を自分で処理できます。**どちらも既定では出ません**（頼まなければ今の playground と同じ見た目です）。

- **閉じる**：いちばん右のボタン。
- **ダウンロード**（アイコン）：その左のボタン。

ボタンを押しても、エディターは自分では閉じたり保存したりしません。組み込んだ側のページに知らせるだけで、何をするか（ダイアログを閉じる、自分の保存先に保存する、など）はページが決めます。

エディターの URL のパラメーター：

| パラメーター | 意味 |
|---|---|
| `view=1` | 読み取り専用モードで起動する（`0`・`false` で編集モード）。メニューバーとツールバーは `view=1` でも既定で表示される。 |
| `menuBar=0` | メニューバーを非表示にする（既定は `menuBar=1` 表示）。 |
| `toolBar=0` | ツールバーを非表示にする（既定は `toolBar=1` 表示）。 |
| `fd=0` | ダウンロードできる書体への誘導（版の初回起動で自動で開くセットアップのダイアログと、ヘルプ > セットアップ）を出さない。既定は出す（`fd=1`）。プレイグラウンドは `fd=0` で起動する。 |
| `close=1` | 閉じるを出す（`0`・`false` で出さない）。 |
| `download=1` | ダウンロードを出す。 |
| `origin=https://app.example.com` | 親の窓へのメッセージの送り先の origin（iframe のときだけ。下を参照）。 |

JavaScript から渡す場合は、`main.js` より**前**に実行されるスクリプトで：

```html
<script>
  window.ibisOfficeOptions = {
    view: true,
    menuBar: false,
    toolBar: false,
    close: true,
    download: true,
    fd: false, // 書体のダウンロードへの誘導を出さない
    targetOrigin: 'https://app.example.com', // iframe のときだけ
    eventTarget: document.getElementById('preview'), // 省略可。既定は window
  };
</script>
<script type="module" src="main.js"></script>
```

JavaScript で渡した値は、URL の同じ指定より優先されます。

## ページが受け取るもの

### 同じページに埋め込んだ場合（iframe のときも、これに加えて）

`window`（または `eventTarget`）に `CustomEvent` が届きます。

```js
window.addEventListener('ibisoffice:close', () => { /* プレビューを閉じる */ });
window.addEventListener('ibisoffice:download', (e) => {
  const { name, bytes, blob } = e.detail; // name: ファイル名、bytes: Uint8Array、blob: Blob
});
```

`download` には、**今の内容**（編集を含む）が、そのファイルの形式のまま入っています（ファイル > 保存 で書かれるのと同じバイト列）。

### iframe の場合

親の窓にはメッセージも届きます。

```js
window.addEventListener('message', (e) => {
  if (e.origin !== 'https://ibisoffice.example.com') return; // エディター側の origin
  const m = e.data;
  if (m?.source !== 'ibisoffice') return;
  if (m.type === 'close') { /* … */ }
  if (m.type === 'download') { /* m.name: 文字列、m.bytes: ArrayBuffer */ }
});
```

メッセージは `origin=` / `targetOrigin` で指定した origin に送ります。指定が無ければ `document.referrer`（組み込んだページ）の origin を使います。どちらも分からないときは**何も送りません**（警告をログに出します）。知らないページにファイルを送らないためです。どのページにも届けたいときだけ、`origin=*`（または `targetOrigin: '*'`）を指定してください。

## ページがファイルを預かる（ホストのインターフェース）

ブラウザー版を埋め込むページは、ファイルを置く場所にもなれます（クラウド ストレージのビューアーなど）。ページ（「ホスト」）が関数をいくつか
ibisOffice に渡し、ibisOffice はその関数だけを呼びます。ibisOffice は、ホストの API・URL・アカウントを知りません。渡すのはバイト列・名前・版
だけで、どう保存するか（ロック・権限・ログイン）はホストが決めます。関数を渡さないホストでは、これまでどおりの動き（プレイグラウンド）です。

ページは `?host=1` を付けて起動します（最小のページ・オプション・関数・イベントは[上](#最小限の設置)にまとめました）。そのための Web のアーカイブのファイルは、`ibisoffice-host.js`（ホストのページで動くブリッジ）と
`ibisoffice-host.d.ts`（型）です。どちらも `ibisOffice-<版>-Web.zip` に入っています。配信する版と同じ版のものを使ってください。

ホストのリポジトリには、使う版のアーカイブから `ibisoffice-host.js` と `ibisoffice-host.d.ts` を同じ名前のまま取り出し、同じフォルダ
（例：`src/office/vendor/`）に置いて取り込みます。手で書き換えず、配信する版を上げるときに、2 つとも同じ版のものへ置き換えてください
（`.js` の先頭に AGPL-3.0-only の著作権表示があり、`.d.ts` にも同じものが付いています）。`attachIframe` が接続できなかったとき
は、`IbisOfficeConnectError`（`ibisoffice-host.js` から export）を投げます。メッセージの文字列ではなく `reason`（`originMismatch`・
`loaderTimeout`・`helloTimeout`・`connectTimeout`・`protocolMismatch`・`refused`）で原因を区別できます。

- **ホストと同じオリジン**（ibisOffice もそのページが配る）：関数をそのまま渡し、直接呼びます。postMessage は使いません。iframe には `sandbox`
  属性を付けず（同じオリジンでスクリプトが動く枠では、外されてしまうため）、`script-src 'self' 'wasm-unsafe-eval'`（`unsafe-eval` なし。
  ibisOffice はこの条件で動きます）と `frame-ancestors 'self'` の Content-Security-Policy で配ってください。
- **別のオリジン**（`attachIframe(iframe, { officeOrigin: 'https://office.example.com', ... })`、`?host=1&origin=<ホストのオリジン>`）：`hello` と
  `connect` のあと、`MessageChannel` のポート 1 本で通信します。バイト列は transfer で渡します。
- 関数はどれも省略できます（[上の表](#ホストが渡す関数host)）。`listFolder` でファイル選択ウィンドウ（開く・名前を付けて保存・書き出し。フォルダはノードを開いたときに 1000 件ずつ
  取ります）、`saveFile` で「保存」（ホストの文書でなければ保存先を尋ねます）、`renameFile` と `getFileLocation` でファイル名のメニュー、
  `openUrl` でリンクを開く操作が使えます。`printPdf` は、`features.print` を偽にしない限りブリッジが既定の実装を渡します。失敗は
  `{ ok: false, reason, message? }` で、`reason` は `locked`・`conflict`・`readOnly`・`noPermission`・`notFound`・`exists`・`tooLarge`・
  `unauthorized`・`network`・`other` です。
- ブラウザー版の ファイル > コピーの保存 は、コピーをすぐにダウンロードします。
- ibisOffice は、リンクを自分では開きません。文書の中のリンクは `openUrl` に渡します（渡すのは `https:`・`http:`・`mailto:` だけで、ホストも
  もう一度確かめてください）。ページの Cookie・`localStorage`・`sessionStorage` は読み書きせず（ホストがあるあいだ、設定はメモリに置きます）、
  Service Worker も登録しません。`scripts/check-web-sources.py` が、ソースと作ったファイルでこれを確かめます。
- `?view=1`・`?menuBar=0`・`?toolBar=0`・`?close=1`・`?download=1`・`?fd=0`・`?origin=` と、上のイベントは今までどおりです。`attachIframe`
  に渡したオプションが、アドレスの同じ項目より優先されます。

## 版を固定する

公開したリリースのファイルは後から変わらないので、確かめた版（[ブラウザー版を入手する](#ブラウザー版を入手する)）はずっと同じです。

- アーカイブの SHA-256 を自分のリポジトリに控えておき、取るたびに比べると確実です。
- 実際に配るファイルを確かめるには、確認の最後のコマンドを配備するフォルダーの中で実行します。
- アーカイブには `license.md`（ブラウザー版の第三者ライセンス表示）と `assets/artborders/ECMA-NOTICE.txt` が入っています。配るときも一緒に置いてください。
- `.wasm` は大きいので、圧縮して送るサーバーだと転送が大幅に小さくなります。
- デスクトップ版はこのファイルを使いません（自動更新は別の仕組みです）。

## wasm を分割して配る（ファイルの大きさに制限のある CDN 向け）

CDN によっては、ファイルの大きさに上限があります（Cloudflare Workers Static Assets は 25 MiB）。`.wasm` はそれより大きいので、`scripts/web-loader-setup.mjs`（Node.js 18 以降。追加のインストールは不要）で分割し、`loader-manifest.json` を書きます。ページは、分割を並列に取り、1 つずつ（大きさと SHA-256 を）確かめてから使い、取りながらアプリを起動します。manifest が無ければ今までどおり 1 ファイルです。リリースの zip は分割していません。配る人が、自分の CDN の上限に合わせて道具を実行します。

```sh
node scripts/web-loader-setup.mjs app --chunk 20 --out app-parts      # 20 MiB 以下の分割（gzip で圧縮）
```

- `--chunk <MiB>` は、保存される 1 つの分割の大きさです。`--encoding gzip`（既定）は、分割の前に gzip で圧縮し、ブラウザーで戻します。CDN が `.part` を圧縮しなくても、通信量は元の約 4 分の 1 です。`--encoding none` はそのまま切ります（CDN の圧縮に任せます）。`--also <パス>` で他のファイルも分割できます。`--out` で、分割と manifest を別のフォルダーに出します（省くと同じ場所。元のファイルは変えないので、zip の SHA-256 の一覧の照合は壊れません）。道具は、書いたものをつなげて戻し、元のファイルと比べてから終わります。
- 分割と `loader-manifest.json` は、`index.html` と同じディレクトリ（分割は `pkg/*.part`）に置きます。分割の名前にはファイル全体の hash が入るので、長くキャッシュしてかまいません。`loader-manifest.json` は毎回再確認するのが向いています。
- manifest が正です。`?chunk=<MiB>`、`ibisOfficeOptions.chunk` は「分割して配っている」と知らせるだけで、manifest と食い違うときは manifest を使い、コンソールに知らせます。`main.js` と版の違う manifest は、起動を止めます（古いものと新しいものの混在）。
- 確認に失敗した分割は、取り直します（3 回。間隔を空けます）。それでも駄目なら起動を止め、状態の行にその分割の名前を出します。`crypto.subtle` が無いページ（`localhost` 以外の http など）では、大きさだけを確かめ、コンソールに警告を出します。

### 書体をチャンクで配る（ブラウザー版は、文書に要るものだけ取る）

セットアップの書体のうち SIL Open Font License のもの（BIZ UD ゴシック・BIZ UD 明朝・ACGJMincho）は、丸ごとでなくチャンクに分けて配れます。`tools/ibis-font-subsetter` が、書体を、基本のチャンク（ASCII と JIS X 0208 の字、約 3 MB）と、残りの字をコードポイント順に切ったチャンク（圧縮後 512 KiB 以下ずつ。`--chunk` で変えられます）に分けます。チャンクはそれぞれ別の書体で、名前は `ibis <id> <key>` です（元の書体の名前は名乗りません。著作者とライセンスの欄は残します）。`web-loader-setup.mjs --subset <フォルダー>` が、チャンクと書体ごとのライセンス・README をページの隣に置き、`loader-manifest.json` に一覧を書きます。ページは、起動のときに各書体の基本のチャンクを、文書が使うほかのチャンクを取ります。チャンクは名前で頼み（文書の字は送りません）、大きさと SHA-256 を確かめ、ブラウザー（IndexedDB）に残します。チャンクが届くまでは、閲覧者の PC の書体で描きます。ホストが `features.fontDownload` を切っているときは、チャンクを取りません。IPAmj 明朝は、ライセンスが分けることを認めないので切りません（セットアップのダウンロードのままです）。

```sh
cargo run --release -p ibis-font-subsetter -- --font BIZUDGothic-Regular.ttf --id BIZUDGothic --typeface "BIZ UDゴシック" --typeface "BIZ UDGothic" --license LICENSE --out blocks
node scripts/web-loader-setup.mjs app --subset blocks --out app-parts
```

## ライセンス

ibisOffice は AGPL-3.0-only と商用ライセンスのデュアルライセンスです。改変した版をネットワーク越しに利用者へ配信する場合、AGPL 第 13 条により、そのソースを利用者に示す必要があります。そのアドレスを `sourceUrl` に渡してください。AGPL の条件なしで使うには、[COMMERCIAL-LICENSE.md](../COMMERCIAL-LICENSE.md) をご覧ください。

## 補足

- デスクトップ版には影響しません。
- ボタンの文字は、エディターの言語（英語・日本語）に従います。
