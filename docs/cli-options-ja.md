<p align="right"><a href="cli-options.md">English</a> | <b>日本語</b></p>

# ibisOffice コマンドラインオプション

ibisOffice は、グラフィカルなオフィススイートとして通常通り起動できるだけでなく、ターミナルやバッチスクリプト、CI/CD パイプラインから利用可能な強力なコマンドラインインターフェース（CLI）を備えています。

画面を表示せずに Word・Excel・PowerPoint ドキュメントを PDF や PNG 画像に高速変換する**ヘッドレス・エクスポート**や、印刷オプションの指定、起動時間のベンチマーク計測などが可能です。

---

## 目次

1. [基本構文](#基本構文)
2. [ファイルを開く・表示オプション](#ファイルを開く表示オプション)
3. [ヘッドレス・エクスポート（--export）](#ヘッドレスエクスポート--export)
4. [印刷・エクスポート詳細オプション](#印刷エクスポート詳細オプション)
   - [Word (.docx) 向けオプション](#word-docx-向けオプション)
   - [PowerPoint (.pptx) 向けオプション](#powerpoint-pptx-向けオプション)
5. [UI スナップショット（--export ui）](#ui-スナップショット--export-ui)
6. [パフォーマンス計測・ベンチマーク](#パフォーマンス計測ベンチマーク)
7. [実用例（レシピ）](#実用例レシピ)

---

## 基本構文

```bash
# 通常の GUI 起動（ファイル指定またはスタートウィンドウ）
ibisOffice [OPTIONS] [FILE]

# ヘッドレス・エクスポート（画面を出さずにファイルを出力して終了）
ibisOffice --export <format> [EXPORT_OPTIONS] <FILE>

# UI スナップショット（画面を出さずにエディタUIのスナップショットを保存）
ibisOffice --export ui [--out-dir <dir>] [--steps <steps>] [<FILE>]

# 起動時間・パフォーマンス測定
ibisOffice [--timing] [--quit-after-first-frame] [<FILE>]
```

### 対応ファイル形式
- **Word 文書**: `.docx`
- **Excel ワークブック**: `.xlsx`
- **PowerPoint プレゼンテーション**: `.pptx`

※ `FILE` を省略して起動した場合は、テンプレート選択や新規作成が可能な「スタートウィンドウ」が開きます。

---

## ファイルを開く・表示オプション

| オプション | 引数 | 説明 |
|---|---|---|
| `FILE` | ファイルパス | 起動時に開く `.docx`, `.xlsx`, `.pptx` ファイルのパス。 |
| `--slide` | `<number>` | 起動時に指定したスライド番号（1 始まり）を直接表示します（PowerPoint 向け）。 |
| `--read-only` | （なし） | 閲覧だけのために開きます。編集はできず、サイドパネルは閉じたままです。 |

**例:**
```bash
# PowerPoint ファイルを開き、最初から 5 枚目のスライドを表示する
ibisOffice --slide 5 presentation.pptx

# 文書を閲覧だけのために開く
ibisOffice --read-only report.docx
```

---

## ヘッドレス・エクスポート（--export）

ウィンドウを一切表示せず、GPU / CPU レンダリングによりバックグラウンドで高速に出力を行い、完了後に即座に終了します。サーバー環境や自動化スクリプトに最適です。

| オプション | 引数 | 説明 |
|---|---|---|
| `--export` | `<format>` | エクスポート形式を指定します（必須）。<br>・`pdf`: ベクターグラフィックスと埋め込みフォントを含む高品質 PDF<br>・`png`: 各ページ/スライドを PNG 画像として連番出力<br>・`pdf-image`: ページを画像化して埋め込んだ PDF<br>・`html`: 文書を 1 つの HTML ページ（グラフと SmartArt は画像）として `--out-dir` に `<名前>.html` で書き出す。表計算の場合は `--range` の最初の番号でシートを選ぶ<br>・`md`（`markdown` も可）: Markdown を `<名前>.md` で書き出す。DOCX・XLSX・PPTX・PDF・XPS・OFD・Markdown・法令 XML・Lawtext のファイルを受け付ける（[固定レイアウト文書を Markdown に書き出す](#固定レイアウト文書を-markdown-に書き出す)を参照）<br>・`txt`（`text` も可）: PDF・XPS・OFD の文字だけを `<名前>.txt` で書き出す（同じ節を参照）<br>・`lawxml`: 法令 XML を `<名前>.xml` で書き出す。入力は `md` と同じ<br>・`lawtext`: Lawtext を `<名前>.lawtext` で書き出す。入力は `md` と同じ<br>・`ui`: エディタの UI 画面のスナップショット |
| `--out-dir` | `<dir>` | 出力先ディレクトリを指定します（デフォルト: カレントディレクトリ `.`）。ディレクトリが存在しない場合は自動作成されます。 |
| `--width` | `<px>` | PNG エクスポート時の画像の横幅ピクセル数を指定します（デフォルト: `1920`）。アスペクト比は自動維持されます。 |
| `--range` | `<range>` | 出力するスライドまたはページの範囲を指定します（例: `1`, `1,3-5`, `2-`）。省略時は全ページ/全スライドが出力されます。以前の名前 `--slides` も使えますが、警告が出ます。 |
| `--include-layout` | （なし） | スライドのレイアウト情報を付加してエクスポートします。以前の名前 `--layout` も使えますが、警告が出ます。 |

**例:**
```bash
# プレゼンテーション全体を PDF に変換
ibisOffice --export pdf presentation.pptx

# 1ページ目から3ページ目までを幅 2560px の高解像度 PNG として dist/ に保存
ibisOffice --export png --out-dir dist/ --width 2560 --range 1-3 document.docx
```

---

## 印刷・エクスポート詳細オプション

Microsoft Office の印刷・エクスポート詳細設定に準拠した出力制御が可能です。

### Word (.docx) 向けオプション

| オプション | 説明 |
|---|---|
| `--print-backgrounds` | 背景の色とイメージ（ページ背景）を出力に含めます（Word の「背景の色とイメージを印刷する」オプション準拠）。 |
| `--print-hidden-text` | 隠し文字を出力に含めます（Word の「隠し文字を印刷する」オプション準拠）。 |
| `--no-print-markup` | 変更履歴やコメント（校閲マークアップ）を出力から除外します（Word の「変更履歴とコメントを印刷する」をオフにする設定準拠）。 |

**例:**
```bash
# 背景画像を含め、校閲マークアップを除外してクリーンな PDF を生成
ibisOffice --export pdf --print-backgrounds --no-print-markup report.docx
```

### PowerPoint (.pptx) 向けオプション

#### カラーモード
| オプション | 引数 | 説明 |
|---|---|---|
| `--print-color` | `color` \| `gray` \| `bw` | 印刷カラーモードを指定します（デフォルト: `color`）。<br>・`color`: カラー印刷<br>・`gray`: グレースケール印刷<br>・`bw`: 単純白黒（ハイコントラスト白黒印刷） |

#### 配布資料（ハンドアウト）設定
| オプション | 引数 | 説明 |
|---|---|---|
| `--handout` | `<n>` | 1ページに複数のスライドを配置する配布資料形式で出力します。<br>指定可能な値: `1`, `2`, `3`, `4`, `6`, `8`, `9`, `16` |
| `--handout-landscape` | （なし） | 配布資料の用紙の向きを「横向き（ランドスケープ）」にします。 |
| `--handout-order` | `horizontal` \| `vertical` | 配布資料のスライドの並び順（デフォルト: `horizontal`。PowerPoint と同じ）。`horizontal` は横に並べてから次の行へ、`vertical` は縦に並べてから次の列へ進みます（1 ページ 4・6・9 枚の場合）。 |
| `--print-frame` | （なし） | 各スライドの外周に細い枠線を付けて出力します（「スライドに枠を付けて印刷」）。 |
| `--print-paper <a3|a4|a5|b4|b5|letter|legal|tabloid|幅x高さmm>` | スライドの大きさ | 各ページをこの用紙に載せます（スライドは縦横から用紙の向きが決まり、ノート・縦の配布資料は縦）。用紙に合わせて拡大縮小し、中央に置きます（余白なし）。 |
| `--no-print-fit-paper` | 用紙に合わせる | `--print-paper` と使います。用紙に合わせず 100% で中央に置きます（はみ出す分は切れます）。 |
| `--notes-pages` | （なし） | スライド画像の下に発表者ノートを配置した「ノート ページ」形式で出力します。 |
| `--outline-pages` | （なし） | スライド内のテキスト階層のみを箇条書きで並べた「アウトライン」形式で出力します。 |

#### 配布資料ヘッダー・フッター
| オプション | 引数 | 説明 |
|---|---|---|
| `--handout-header` | `<text>` | 配布資料のヘッダー文字列を指定します。 |
| `--handout-date` | `<text>` | 配布資料の日付欄の文字列を指定します（デフォルト: 今日の日付。配布資料マスターのヘッダーとフッターの設定で日付がオフなら出しません）。 |
| `--handout-footer` | `<text>` | 配布資料のフッター文字列を指定します。 |
| `--handout-number` | （なし） | 配布資料にページ番号を印刷します（デフォルトでオン。配布資料マスターのヘッダーとフッターの設定でオフなら出しませんが、このオプションを付けると出します）。 |
| `--no-handout-date` | （なし） | 配布資料・ノート・アウトラインに日付を印刷しません。 |
| `--no-handout-number` | （なし） | 配布資料・ノート・アウトラインにページ番号を印刷しません。 |

**例:**
```bash
# 1ページに3スライド配置し、枠線付き・横向き・ヘッダー・日付・ページ番号入りで PDF 出力
ibisOffice --export pdf --handout 3 --handout-landscape --print-frame \
  --handout-header "社内勉強会資料" --handout-date "2026-10-16" --handout-number \
  presentation.pptx

# 発表者ノート付き PDF を生成
ibisOffice --export pdf --notes-pages presentation.pptx
```

---

## UI スナップショット（--export ui）

ヘッドレス環境で ibisOffice のエディタ UI 自体の描画状態をスナップショット（PNG）として記録します。UI のレイアウト検証や自動化テストで使用されます。

| オプション | 引数 | 説明 |
|---|---|---|
| `--steps` | `<steps>` | 実行する UI アクションをカンマ区切りで指定します（デフォルト: `start`）。<br>例: `start,pagedown,end` |

**例:**
```bash
# スタートウィンドウの UI 画面を output/ にキャプチャ
ibisOffice --export ui --out-dir output/

# ドキュメントを開いた状態のエディタ画面をキャプチャ
ibisOffice --export ui --out-dir output/ presentation.pptx
```

---

## パフォーマンス計測・ベンチマーク

ibisOffice の高速な起動処理・レンダリング各ステージの所要時間を精密に測定するためのオプションです。

| オプション | 説明 |
|---|---|
| `--timing` | 起動各ステージ到達時のミリ秒（プロセス開始からの経過時間）を標準エラー出力（stderr）に `ibisOffice timing: <stage> <ms>` 形式で出力します。 |
| `--quit-after-first-frame` | ドキュメントおよびウィンドウの初回描画が完了（第2フレーム安定描画後）した時点で、自動的にプロセスを正常終了します。 |

### 計測されるステージ一覧
- `main`: `main()` 関数の実行開始
- `app`: アプリケーションループ・プラットフォーム層初期化完了
- `window`: ウィンドウ生成完了
- `first-frame`: ウィンドウの初回フレーム描画完了
- `second-frame`: 第2フレーム描画完了
- `read`: ファイルデータのディスク読み込み完了（非同期スレッド）
- `parsed`: OOXML（ドキュメントモデル）解析完了
- `loaded`: レイアウト計算完了・ウィンドウへのドキュメント読み込み完了
- `document-frame`: ドキュメント内容の初回描画完了
- `document-second-frame`: ドキュメント内容の第2フレーム描画完了
- `slide-pictures`: 表示中スライドの画像デコード完了
- `all-pictures`: ドキュメント内全画像デコード・サムネイル生成完了

**例:**
```bash
# 起動時間を計測し、描画完了後に自動終了する
ibisOffice --timing --quit-after-first-frame presentation.pptx
```

出力例:
```text
ibisOffice timing: main 0.12
ibisOffice timing: app 12.45
ibisOffice timing: window 28.30
ibisOffice timing: read 31.10
ibisOffice timing: parsed 38.50
ibisOffice timing: loaded 45.20
ibisOffice timing: first-frame 48.90
ibisOffice timing: document-frame 52.10
ibisOffice timing: document-second-frame 68.40
```

---

## 実用例（レシピ）

### 1. ドキュメントの一括 PDF 変換（CI/CD・バッチ処理）
```bash
for file in docs/*.docx slides/*.pptx; do
  ibisOffice --export pdf --out-dir dist/ "$file"
done
```

### 2. Web 掲載用の高解像度サムネイル画像の自動生成
```bash
# スライド 1 枚目を幅 1920px の PNG として書き出し
ibisOffice --export png --range 1 --width 1920 --out-dir thumbnails/ presentation.pptx
```

### 3. 会議用 2 in 1 白黒ハンドアウト PDF の作成
```bash
ibisOffice --export pdf --handout 2 --print-color bw --print-frame \
  --handout-header "2026年10月度 経営戦略会議" \
  --handout-footer "アイビス株式会社 機密" \
  --handout-number --out-dir handouts/ strategy.pptx
```

## 固定レイアウト文書を Markdown に書き出す

```sh
ibisOffice --export md --out-dir ./out input.pdf
```

PDF・XPS（OXPS を含む）・OFD の抽出可能な本文をページ順に書き出します。パスワードなしで開ける暗号化 PDF（所有者が印刷・複写だけを制限したもの。Standard セキュリティハンドラの RC4・AES-128・AES-256）は復号して読みます。利用者パスワードが必要な PDF はその旨を報告します。行の順序は文字の位置から推定します。ビューアで復号できた図形・画像は、ページ上の配置を持つ SVG として本文に添えます。画像だけのページは OCR を行いません。段落や表の意味構造、署名、原本と同じ組版の復元には別の処理が必要です。DOCX の EMF/WMF は Markdown 内に元の画像データを保持します。一般のブラウザーでの EMF/WMF 表示は閲覧環境に依存します。

表の列位置を保つには `--text-layout physical` を付けます。各ページの本文を ```` ```text ```` ブロックに入れ、文字を紙面上の位置に応じた桁に置きます（半角 1 桁・全角 2 桁、`pdftotext -layout` 相当）。Markdown のエスケープはしません。既定の `--text-layout reading` は、各行を語の間を空白 1 つで区切った Markdown として書き出します。

```sh
ibisOffice --export md --text-layout physical --out-dir ./out input.pdf
```

本文だけが要るときは `--export txt` を使います。各ページの行をそのまま書き、ページの終わりに改ページ文字（form feed、`pdftotext` と同じ）を置きます。Markdown のエスケープも図形も付けません。描画や画像の展開をしないので `--export md` より速く終わります。`--text-layout physical` も使えます。

```sh
ibisOffice --export txt --text-layout physical --out-dir ./out input.pdf
```
