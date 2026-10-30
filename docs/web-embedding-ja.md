# ブラウザー版の組み込み

[English](web-embedding.md) | **日本語**

ブラウザー版（`www/`）は、クラウドサービスのファイルのプレビューなど、ほかのページの中に置けます。窓の右上に 2 つのボタンを出せば、組み込んだ側のページが「閉じる」と「ダウンロード」を自分で処理できます。**どちらも既定では出ません**（頼まなければ今の playground と同じ見た目です）。

- **閉じる**：いちばん右のボタン。
- **ダウンロード**（アイコン）：その左のボタン。

ボタンを押しても、エディターは自分では閉じたり保存したりしません。組み込んだ側のページに知らせるだけで、何をするか（ダイアログを閉じる、自分の保存先に保存する、など）はページが決めます。

## ボタンを出す

エディターの URL のパラメーター：

| パラメーター | 意味 |
|---|---|
| `view=1` | 読み取り専用モードで起動する（`0`・`false` で編集モード）。メニューバーとツールバーは `view=1` でも既定で表示される。 |
| `menuBar=0` | メニューバーを非表示にする（既定は `menuBar=1` 表示）。 |
| `toolBar=0` | ツールバーを非表示にする（既定は `toolBar=1` 表示）。 |
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

## 版を固定して取り、確かめる

リリースごとに、ブラウザー版をまとめたアーカイブを添付します。配る版を固定でき、取ってきたファイルが壊れていないか・差し替えられていないかを SHA-256 で確かめられます。中身は playground の `app/` と同じです（`index.html`・`main.js`・`license.md`・`pkg/`・`assets/`。アーカイブの直下に並びます）。

| ファイル（`<版>` は `0.8.5` など） | 内容 |
|---|---|
| `ibisOffice-<版>-Web.zip` | ブラウザー版 |
| `ibisOffice-<版>-Web.zip.sha256` | アーカイブの SHA-256 |
| `ibisOffice-<版>-Web.files.sha256` | アーカイブの中の各ファイルのパスと SHA-256 |

3 つとも、リリースのページ `https://github.com/ibisjp/ibis-office/releases/tag/v<版>` にあります。取ってきて、アーカイブを照合し、展開して、ファイルごとに照合します。

```sh
V=0.8.5
BASE=https://github.com/ibisjp/ibis-office/releases/download/v$V
curl -LO $BASE/ibisOffice-$V-Web.zip
curl -LO $BASE/ibisOffice-$V-Web.zip.sha256
curl -LO $BASE/ibisOffice-$V-Web.files.sha256

shasum -a 256 -c ibisOffice-$V-Web.zip.sha256        # macOS（Linux は sha256sum -c ...）
mkdir app && unzip -q ibisOffice-$V-Web.zip -d app
(cd app && shasum -a 256 -c ../ibisOffice-$V-Web.files.sha256)   # すべての行が OK になること
```

Linux では同じファイルを `sha256sum -c` に渡せます。Windows の PowerShell では、`(Get-FileHash ibisOffice-$V-Web.zip -Algorithm SHA256).Hash` の値を `.sha256` ファイルのハッシュと比べます（大文字小文字は区別しません）。実際に配るファイルを確かめるには、最後のコマンドを配備するフォルダーの中で実行してください。

補足：

- 公開したリリースのファイルは、後から変わりません。確かめた版はずっと同じです。アーカイブの SHA-256 を自分のリポジトリに控えておき、取るたびに比べると確実です。
- アーカイブには `license.md`（ブラウザー版の第三者ライセンス表示）と `assets/artborders/ECMA-NOTICE.txt` が入っています。配るときも一緒に置いてください。
- フォルダーはそのまま配ってください。`.wasm` は大きい（約 65 MB）ので、圧縮して送るサーバーだと転送が大幅に小さくなります。
- デスクトップ版はこのファイルを使いません（自動更新は別の仕組みです）。

## 補足

- デスクトップ版には影響しません。
- ボタンの文字は、エディターの言語（英語・日本語）に従います。
