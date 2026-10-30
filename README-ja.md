<p align="center"><a href="README.md">English</a> | <b>日本語</b></p>

<p align="center">
  <img src="asset/ibisOffice_svg_logo_for_light.svg" alt="ibisOffice" width="480">
</p>

<p align="center">
  <b>Rust と GPU で作った、Microsoft Office と同じ見た目で開ける次世代のオフィス スイート</b><br>
  Word（.docx）・Excel（.xlsx）・PowerPoint（.pptx）を、速く、正確に、どこでも。
</p>

---

<p align="center">
  <img src="asset/screenshots.png" alt="ibisOffice で開いた Word・Excel・PowerPoint の文書" width="100%">
</p>

## ibisOffice とは

ibisOffice は、Word・Excel・PowerPoint の文書を、表示し、編集し、保存するためのオフィス スイートです。すべてを Rust で書き、描画は GPU で行います。macOS・Windows・Linux のデスクトップでも、Web ブラウザでも、同じ 1 つのコードで動きます。

目指しているのは、「Microsoft Office で作った文書を開いても、見た目が崩れない」ことです。行の折り返し、ページの区切り、表の罫線、グラフの目盛りまで、Office と同じ結果になるように作っています。

## すぐに始める

### デスクトップアプリ版をダウンロード

<table>
  <tr>
    <td align="center">
      <a href="https://github.com/ibisjp/ibis-office/releases/download/v0.8.5/ibisOffice-0.8.5-Windows.zip">
        <img src="asset/download-windows.svg" alt="Download for Windows" height="54">
      </a><br>
      <a href="https://github.com/ibisjp/ibis-office/releases/download/v0.8.5/ibisOffice-0.8.5-Windows.zip"><code>ibisOffice-0.8.5-Windows.zip</code></a><br>
      <small>Windows 10 / 11（64 ビット）</small>
    </td>
    <td align="center">
      <a href="https://github.com/ibisjp/ibis-office/releases/download/v0.8.5/ibisOffice-0.8.5-macOS.zip">
        <img src="asset/download-mac.svg" alt="Download for Mac" height="54">
      </a><br>
      <a href="https://github.com/ibisjp/ibis-office/releases/download/v0.8.5/ibisOffice-0.8.5-macOS.zip"><code>ibisOffice-0.8.5-macOS.zip</code></a><br>
      <small>macOS 11 以降（Apple Silicon）</small>
    </td>
    <td align="center">
      <a href="https://github.com/ibisjp/ibis-office/releases/download/v0.8.5/ibisOffice-0.8.5-Linux.AppImage">
        <img src="asset/download-linux.svg" alt="Download for Linux" height="54">
      </a><br>
      <a href="https://github.com/ibisjp/ibis-office/releases/download/v0.8.5/ibisOffice-0.8.5-Linux.AppImage"><code>ibisOffice-0.8.5-Linux.AppImage</code></a><br>
      <small>Linux x86_64</small>
    </td>
  </tr>
</table>

- **Windows**：`ibisOffice-0.8.5-Windows.zip` をダウンロードし、展開した `ibisOffice` フォルダの `ibisOffice.exe` を起動してください。
- **macOS**：`ibisOffice-0.8.5-macOS.zip` をダウンロードし、展開した `ibisOffice.app` をアプリケーション フォルダに移してください。
- **Linux**：`ibisOffice-0.8.5-Linux.AppImage` をダウンロードし、実行権限を付けて（`chmod +x`）、ダブルクリックしてください。主要なデスクトップ Linux（glibc 2.31 以降：Ubuntu、Debian、Fedora、Arch Linux など）で動作します。インストールも FUSE も不要です。
- **Web 版を試す**：インストールせずに、ブラウザで [playground](https://ibisjp.github.io/ibis-office/) を開いてください（WebGPU に対応したブラウザ）。見本の文書（Word・Excel・PowerPoint）を開いて、すぐに試せます。
- **更新履歴**：各バージョンの変更内容は [CHANGELOG.md](CHANGELOG.md) をご覧ください。
- **動画で見る**：ibisOffice の紹介動画（YouTube）。

  [![ibisOffice の紹介動画](asset/video-ja.jpg)](https://www.youtube.com/watch?v=0yjHdK_Cb_8)

- **コマンドライン操作**：ターミナルからの起動、ヘッドレスでの PDF/PNG 一括変換、印刷設定、起動ベンチマーク測定については [コマンドラインオプション](docs/cli-options-ja.md) をご覧ください。
- **ブラウザー版の組み込み**：閉じる・ダウンロードのボタンを出し、組み込んだ側のページにイベントで知らせる方法は [ブラウザー版の組み込み](docs/web-embedding-ja.md) をご覧ください。

## ゴール

ibisOffice は、世界でトップクラスのオープンソースのオフィス スイートになることを目指しています。

- **速さと堅牢さ**：Rust によるメモリ安全なコードで、大きな文書もすばやく開いて、落ちない。
- **Microsoft Office と同じ見た目**：文字の位置、改行、改ページ、表、グラフ、図形を、Office と同じに描く。
- **Microsoft Office と同じファイル**：開いて保存しても、Office で開き直したときに中身も見た目も失われない。
- **OS を選ばない**：デスクトップアプリ版は、macOS・Windows・Linux で、同じ操作と同じ見た目。
- **ブラウザで、ネイティブと同じ速さ**：WebAssembly と WebGPU で、インストールせずに使える。
- **共同編集**：サーバー版で、複数の人が同じ文書を同時に編集できる。

## 特長

### 3 つの形式を 1 つのアプリで

- **文書（.docx）**：段落とスタイル、箇条書きと段落番号、表、段組み、縦書き、ルビ・日本語組版（禁則処理・縦中横・組み文字・囲み文字）、脚注と文末脚注、ヘッダーとフッター、フィールド（ページ番号・目次・索引・相互参照）、数式（OMML・LaTeX・UnicodeMath）、図形・画像・グラフ・SmartArt・インク注釈、変更履歴とコメント、差し込み印刷、コンテンツコントロール、文書の保護、ページ設定、他形式変換（.doc・.rtf・.odt・.html・.txt・.pdf）。
- **表計算（.xlsx）**：500 種類以上のワークシート関数、動的配列とスピル、`LET`・`LAMBDA`、構造化参照・3D 参照、条件付き書式（データバー・カラースケール・アイコンセット）、表示形式（和暦・カスタム書式を含む）、セル内の部分書式とふりがな（ルビ）、オートフィルター、複数列の並べ替え、テーブル、ピボットテーブルとピボットグラフ、スライサーとタイムライン、2D/3D および Office 2016+ 拡張グラフ（ウォーターフォール・ツリーマップ・サンバースト・ヒストグラム・パレート・箱ひげ図・ファンネル）、スパークライン、フォームコントロール、インク注釈、データの入力規則、フラッシュフィル、統合、小計、What-If 分析（ゴールシーク・データテーブル・シナリオ・予測シート・ソルバー）、Power Query データテーブル、スレッド形式コメントとメモ、シートとブックの保護、他形式変換（.xls・.ods・.csv・.tsv・.txt・.html・.dif・.slk・.pdf）。
- **プレゼンテーション（.pptx）**：スライドマスター・レイアウト・テーマとバリエーション・カスタムスライドサイズ・セクション・ヘッダーとフッター・発表者ノートと配布資料、図形（ECMA-376 全 187 種のプリセット図形・図形の結合・頂点の編集・3D 回転・面取り・奥行き）、ワードアートと文字の効果、数式、表、グラフ、SmartArt、画像（図形に合わせてトリミング・アート効果・背景の削除）、3D モデル、動画・音声（トリミング・フェード・ブックマーク・字幕）、インク注釈、動作設定ボタンとズームリンク、アニメーション（開始・強調・終了・軌跡・トリガー・グラフ/SmartArt 要素別ビルド）、画面切り替え（変形［Morph］・効果音を含む）、スライドショーと発表者ビュー（2 画面対応・テレプロンプター・レーザーポインターとペン描画）、目的別スライドショー、リハーサルと録画プレビュー、コメント、プレゼンテーションの比較、アクセシビリティ検査、パスワード暗号化、他形式変換・書き出し（.ppt・.odp・.ppsx・.potx・.pdf・.png・アニメーション .gif）。
- **マクロ付きのファイル**（.docm・.xlsm・.pptm）も開いて保存できます。マクロは実行せず、そのまま残します。

### Office と同じ見た目を、測って確かめる

見た目の互換は、感覚ではなく測定で確かめています。機能ごとに試験用の文書を作り、本物の Word・Excel・PowerPoint が書き出した PDF と ibisOffice の描画を、画素の単位で比べています。保存したファイルは、Office で修復なしに開けることを確かめています。

### GPU で描く、速い画面

画面は GPU で描きます（macOS は Metal、Windows は DirectX、Linux は Vulkan、ブラウザは WebGPU）。スクロール、拡大・縮小、シートの切り替えが滑らかです。UI は、高速なエディター Zed のために作られた UI フレームワーク GPUI で作っています。

### デスクトップでもブラウザでも

同じ Rust のコードが、ネイティブのアプリとしても、WebAssembly としてブラウザの中でも動きます。

## 強み

- **ゼロから Rust で書いた、新しい実装**：既存のオフィス スイートの古いコードを引き継いでいません。メモリ安全で、並列に強く、速い。
- **描画の互換を、仕組みで保証する**：7 万を超える試験で、Office の出力と画素の単位で比べ続けています。
- **編集はすべて「コマンド」**：すべての変更は、ID と確定した値だけを持つコマンドを通ります。これは、元に戻す・やり直しの単位であり、共同編集でサーバーとやり取りする単位にもなります。
- **規格に忠実**：Office Open XML の国際標準（ECMA-376 / ISO/IEC 29500 / JIS X 4191）に沿って読み書きします。図形の輪郭などの組み込みの定義は、Ecma が配布する ECMA-376 の公式のデータをそのまま使っています。
- **商用でも安心して使える部品だけ**：依存するライブラリは、Apache-2.0・MIT・BSD などの商用で使えるライセンスのものだけです。コピーレフトのライブラリは使っていません（`cargo deny` で毎回確かめています）。

## 動機

私たちは、Google Drive のようなファイルの保存・共有のサービス ibisStorage を提供しています。そこでは、Office のファイルのオンラインでのプレビューと編集を、速く、正確に、サーバーに負荷をかけずに行えるようにする必要があります。これが ibisOffice を作る動機です。

## 速さ

小さいほど速く、かっこ内は ibisOffice の何倍かかったかを表します。

### ファイルを開いて最初の画面が出るまで

アプリを起動しておき、ファイルを開かせてから、1 ページ目（pptx は 1 枚目のスライド、xlsx は 1 シート目の最初の画面）が完全に表示されるまでの時間です。

<p align="center">
  <img src="asset/benchmark-open-ja.svg" alt="ファイル表示速度ベンチマーク: ibisOffice vs Microsoft Office, LibreOffice, ONLYOFFICE" width="100%">
</p>

| ファイル | ibisOffice | Microsoft Office | LibreOffice | ONLYOFFICE |
|---|---:|---:|---:|---:|
| プレゼンテーション（.pptx）<br>31 スライド、35.9MB | **0.59 秒** | 2.16 秒（3.7 倍） | 4.40 秒（7.5 倍） | 4.75 秒（8.1 倍） |
| 文書（.docx）<br>英文 140〜154 ページ、0.12MB | **0.45 秒** | 1.11 秒（2.5 倍） | 1.70 秒（3.8 倍） | 3.15 秒（7.0 倍） |
| 表計算（.xlsx）<br>22 シート、グラフ 29 個、数式 58.9 万個、13.7MB | **0.52 秒** | 14.07 秒（27.0 倍） | 6.82 秒（13.1 倍） | 9.36 秒（17.9 倍） |

アプリのウィンドウを録画し（最大 60 フレーム/秒）、ファイルを開かせた時点から画面の変化が止まるまでを、動画から求めました。終わりのフレームは目で確かめています。

### 100万行のCSVファイルを開いて最初の画面が出るまで（First View）

アプリを起動しておき、100万行のCSVファイルを開かせてから、最初の画面が表示され、閲覧・スクロール・ズームが可能になるまでの時間です。

<p align="center">
  <img src="asset/benchmark-csv-ja.svg" alt="100万行 CSV 初回表示（First View）速度ベンチマーク: ibisOffice vs Microsoft Excel, LibreOffice, ONLYOFFICE" width="100%">
</p>

| ファイル | ibisOffice | LibreOffice | Microsoft Excel | ONLYOFFICE |
|---|---:|---:|---:|---:|
| 巨大表計算（.csv）<br>1,048,576行、20列、116MB<br>*(初回画面表示 / First View)* | **0.78 秒** | 11.30 秒（14.5 倍） | 19.04 秒（24.4 倍） | 137.3 秒※（176.0 倍） |

※ ONLYOFFICE は 100 万行のファイルを開けなかった（5 分でタイムアウト）ため、30 万行が 41.60 秒なので、その 3.3 倍と推定した値です。

### PDF への変換

コマンドラインで PDF に変換し、コマンドを始めてから PDF を書き終えるまでの時間です。

<p align="center">
  <img src="asset/benchmark-pdf-ja.svg" alt="PDF 変換速度ベンチマーク: ibisOffice vs Microsoft Office, LibreOffice, ONLYOFFICE" width="100%">
</p>

| ファイル | ibisOffice | Microsoft Office | LibreOffice | ONLYOFFICE |
|---|---:|---:|---:|---:|
| プレゼンテーション（.pptx）<br>31 スライド、35.9MB | **1.42 秒** | 8.40 秒（5.9 倍） | 32.26 秒（22.8 倍） | 5.87 秒（4.1 倍） |
| 文書（.docx）<br>英文 140〜154 ページ、0.12MB | **0.69 秒** | 12.49 秒（18.1 倍） | 3.45 秒（5.0 倍） | 4.19 秒（6.1 倍） |
| 表計算（.xlsx）<br>22 シート、グラフ 29 個、数式 58.9 万個、13.7MB | **7.26 秒** | 42.94 秒（5.9 倍） | 19.46 秒（2.7 倍） | 24.89 秒（3.4 倍） |

変換に使ったもの：ibisOffice は `ibisOffice --export pdf`、Microsoft Office は AppleScript で開いて PDF で保存、LibreOffice は `soffice --headless --convert-to pdf`、ONLYOFFICE はアプリに入っている変換プログラム x2t。

### 計測の条件

- 計測環境：Apple M2 Pro、メモリ 16GB、macOS 26.5.1。製品の版：ibisOffice 0.1.0（開発版）、Microsoft Office 16.113.2、LibreOffice 26.8.0.3、ONLYOFFICE 9.4.0。計測日：2026 年 9 月 29〜30 日。
- 各 1 回の計測です。どの計測も、そのアプリのプロセスがすべて終了した状態から始めました。
- 使ったファイルは、架空のデータと、メタ情報を取り除いた公開の資料から作ったもので、`test/benchmark/` で公開しています。

## 用途

### デスクトップアプリ（macOS・Windows・Linux）

- **毎日のオフィス ソフトとして**：Word・Excel・PowerPoint の文書を作り、編集し、Office と同じ形式で保存する。
- **受け取った文書をすぐ確かめる**：メールやチャットで届いた文書を、待たずに開いて、Office で作ったとおりの見た目で読む。大きなブックや、スライドの多いプレゼンテーションも速く開く。
- **PDF・画像への書き出し**：画面を出さずに、コマンドラインから PDF や PNG に書き出す。文書の変換をまとめて行う処理や、サーバーでの自動処理に組み込める。

### Web 版（WebAssembly ＋ WebGPU）

- **インストールせずに使う**：ブラウザで開くだけで、デスクトップアプリと同じ画面・同じ見た目で文書を表示し、編集する。ソフトを入れられない PC や、共有の端末でも使える。
- **Web システムに文書の表示と編集を組み込む**：業務システム・SaaS・ポータルの中で、添付された Word・Excel・PowerPoint の文書を、その場で Office と同じ見た目で表示・編集する。
- **文書をサーバーに送らずに扱う**：描画も編集もブラウザの中で行うので、手元のファイルを開くときは、文書の中身を外部のサーバーに送らない。

## ビルド

```bash
git clone --recurse-submodules https://github.com/ibisjp/ibis-office.git
cd ibis-office
cargo build --release
```

- Windows では `cargo build --release -p ibis-office-app` でビルドします（ワークスペース全体には Linux 専用の crate が含まれるため）。アプリは `target\release\ibisOffice.exe` にできます。
- macOS のアプリは `scripts/bundle-macos.sh --release` で、`target/release/ibisOffice.app` に作られます。
- Linux では `scripts/bundle-linux.sh --release` で AppImage が `target/dist/ibisOffice-<版>-Linux.AppImage` に、`scripts/bundle-flatpak.sh --release` で Flatpak が `target/dist/ibisOffice-<版>-Linux-<アーキテクチャ>.flatpak` に作られます（Flatpak には `flatpak-builder` が要ります）。
- ブラウザ版は `scripts/build-web.sh --release` でビルドし、`www/` を配信します（WebGPU に対応したブラウザ：Chrome・Edge・Safari 26 以降）。

## 実装済み機能

ibisOffice は、Word (.docx)・Excel (.xlsx)・PowerPoint (.pptx) の 3 大形式について、ECMA-376 規格に準拠した広範な機能を実装しています。

全フォーマットにわたる詳細な実装済み機能の一覧（1,800 項目以上）は、以下をご覧ください：

<p align="center">
  👉 <b><a href="docs/implemented-feature-ja.md">実装済み機能一覧</a></b>
</p>

## ライセンス

ibisOffice は、**AGPL-3.0-only と商用ライセンスのデュアルライセンス**です。

- **AGPL-3.0**：オープンソースとして、自由に使い、改変し、配布できます。改変したものをネットワーク越しにサービスとして提供する場合も、そのソースを AGPL-3.0 で公開する必要があります。本文は [LICENSE](LICENSE) にあります。
- **商用ライセンス**：AGPL-3.0 の条件（ソースの公開）を満たせない製品やサービスに組み込む場合は、商用ライセンスをご利用ください。案内は [COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md) にあります。

ibisOffice の著作権は、すべて株式会社アイビスが持っています。そのため、両方のライセンスで提供できます。

使っている部品と、それぞれのライセンスが求める表示は [THIRD-PARTY-NOTICES.md](docs/THIRD-PARTY-NOTICES.md) にあります（ビルドごとに [www/license.md](www/license.md)・[crates/ibis-office-app/macos/license.md](crates/ibis-office-app/macos/license.md)・[crates/ibis-office-app/linux/license.md](crates/ibis-office-app/linux/license.md)）。GPUI（Apache-2.0）は、改変したフォークを [gpui-pre](https://github.com/ibisjp/gpui-pre)・[gpui-pre-macos](https://github.com/ibisjp/gpui-pre-macos)・[gpui-pre-windows](https://github.com/ibisjp/gpui-pre-windows)・[gpui-pre-linux](https://github.com/ibisjp/gpui-pre-linux)・[gpui-pre-wgpu](https://github.com/ibisjp/gpui-pre-wgpu)・[gpui-pre-web](https://github.com/ibisjp/gpui-pre-web) で公開しています。

## 貢献について

不具合の報告・要望・修正の案は、Issues で歓迎します（今はプル リクエストを受け付けていません）。コードを含む issue は、[CLA](docs/CLA-ja.md) への 1 行の返信で取り込めるようになります。くわしくは [CONTRIBUTING.md](docs/CONTRIBUTING.md) を見てください。セキュリティの問題は、Issues に書かず [SECURITY.md](docs/SECURITY.md) の窓口から知らせてください。

## 商標について

Microsoft、Microsoft Office、Microsoft 365、Word、Excel、PowerPoint は、米国 Microsoft Corporation の米国およびその他の国における登録商標または商標です。LibreOffice は The Document Foundation の、ONLYOFFICE は Ascensio System SIA の登録商標または商標です。その他、記載されている会社名・製品名は、各社の登録商標または商標です。ibisOffice は、これらの会社とは関係がなく、承認や後援を受けたものではありません。

「ibisOffice」の名前とロゴは株式会社アイビスの商標です。使い方は [TRADEMARK.md](docs/TRADEMARK.md) をご覧ください。ibisOffice をもとにしたものには「Powered by ibisOffice」の表示が必要です。

---

© 株式会社アイビス（ibis inc.）
