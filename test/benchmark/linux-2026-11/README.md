# Linux 版のベンチマーク（2026 年 11 月、プレスリリース用）

ibisOffice 0.8.5・LibreOffice 26.8.2・ONLYOFFICE 10.0.0 を、同じ Linux の PC で比べた記録です。Microsoft Office は Linux 版が無いので入れていません。報告は [report.html](report.html)、グラフは `benchmark-{open,csv,pdf}-ja.svg`（`publish/README-ja.md` のグラフと同じ形）。

| ファイル | 中身 |
|---|---|
| `report.html` | 報告（結果・条件・終わりのフレーム） |
| `benchmark-open-ja.svg`・`benchmark-csv-ja.svg`・`benchmark-pdf-ja.svg` | グラフ |
| `results.json` | 3 回ずつの値と中央値 |
| `overrides.json` | 計測できなかった回と、その理由（ONLYOFFICE の pptx のエラーのダイアログ、ONLYOFFICE の 100 万行・30 万行の CSV） |
| `runs/*.json` | 1 回ずつの結果（`open-*`：ファイルを開く、`csv-*-csv-*`：100 万行の CSV の First View、`csv-onlyoffice-csv300k-*`・`csv-onlyoffice-csv100k-*`：ONLYOFFICE の 30 万行・10 万行の CSV、`pdf-*`：PDF への変換） |
| `runs/frames/<回>/` | 終わりと判定したフレーム（`end.png`）、その直前（`before-end.png`）、録画の最後（`last.png`）。アプリの窓の部分だけ（影は除く）、半分の大きさ |
| `runs/raw/` | 録画・取り込みの記録・書き出した PDF（大きいので git に入れない） |

## 環境

- Intel Core i7-10700K（8 コア 16 スレッド）、メモリ 16GB、NVMe SSD、Ubuntu 20.04.6 LTS、GNOME Shell 3.36.9（X11）。GPU は NVIDIA GeForce RTX 2080 Ti（ドライバー 575.57.08）。画面は xrdp のリモートデスクトップ（1920×1080）で、X サーバー（xorgxrdp）は GPU を使わない。描画に GPU を使うのは ibisOffice だけ（Vulkan で NVIDIA、`nvidia-smi` で確認。描いた画像は GPU から読み戻して X サーバーへ渡す）で、LibreOffice と ONLYOFFICE は CPU で描く。ほかの利用者もログインしている共用の PC。
- アプリは管理者の権限なしに `~/opt/bench-apps/` に入れた：
  - ibisOffice 0.8.5：GitHub のリリースの `ibisOffice-0.8.5-Linux.AppImage`（SHA-256 を照合）。
  - LibreOffice 26.8.2.2：download.documentfoundation.org の `LibreOffice_26.8.2_Linux_x86-64_deb.tar.gz`（SHA-256 を照合）の deb を `dpkg-deb -x` で展開。26.8 は glibc 2.34 以上が要り、Ubuntu 20.04（glibc 2.31）では起動しないので、Ubuntu 22.04 の公式のパッケージ `libc6_2.35-0ubuntu3.15` と `libstdc++6_12.3.0-1ubuntu1~22.04.3` を展開し、そのローダーで `soffice.bin` を起動する（`~/opt/bench-apps/bin/soffice26`、プロファイルは `~/opt/bench-apps/lo-profile`）。
  - ONLYOFFICE Desktop Editors 10.0.0：GitHub のリリースの `onlyoffice-desktopeditors-x64.tar.xz` を展開（`~/opt/bench-apps/bin/onlyoffice10` は公式の `/usr/bin/onlyoffice-desktopeditors` と同じく `LD_LIBRARY_PATH` を足して起動する）。
- 設定：
  - LibreOffice：窓 1280×800、初回のダイアログ（Welcome・今日のヒント・寄付の案内）なし、自動回復・自動保存なし（毎回プロセスを終了させるので、そのままでは次の起動で「ドキュメントの回復」が出る）。この設定の `registrymodifications.xcu` を `~/opt/bench-apps/lo-registry-bench.xcu` に置き、毎回の起動の前に写す。
  - ONLYOFFICE：`~/.config/onlyoffice/DesktopEditors.conf` に `position=@Rect(293 115 1334 850)`（影を除いて約 1280×800）。
  - ibisOffice：既定のまま（窓 1280×800）。
  - 計測中は GNOME の画面のロック（`org.gnome.desktop.session idle-delay`）を止めた（終わってから 300 秒に戻した）。

## 計り方

`scripts/benchmark-linux/`（`bench.py` が計測、`xbench.c` が画面の取り込み、`report.py` がグラフと報告）。

```sh
gcc -O2 -o target/tools/xbench scripts/benchmark-linux/xbench.c -lX11 -lXext /usr/lib/x86_64-linux-gnu/libXtst.so.6
python3 scripts/benchmark-data/gen_csv.py test/benchmark/kpi-max-rows.csv
python3 scripts/benchmark-data/gen_csv.py test/benchmark/kpi-300k-rows.csv --rows 300000
python3 scripts/benchmark-data/gen_csv.py test/benchmark/kpi-100k-rows.csv --rows 100000
export DISPLAY=:12
O=test/benchmark/linux-2026-11/runs
for k in docx pptx xlsx; do for a in ibisoffice libreoffice onlyoffice; do
  python3 scripts/benchmark-linux/bench.py $O open $a $k 3
  python3 scripts/benchmark-linux/bench.py $O pdf $a $k 3
done; done
for a in ibisoffice libreoffice onlyoffice; do python3 scripts/benchmark-linux/bench.py $O csv $a csv 3; done
python3 scripts/benchmark-linux/bench.py $O csv onlyoffice csv300k 3
python3 scripts/benchmark-linux/bench.py $O csv onlyoffice csv100k 3
python3 scripts/benchmark-linux/report.py test/benchmark/linux-2026-11
```

- 画面：アプリの窓を 1 秒に最大 100 回取り込み、Mac の `scripts/benchmark-analyze.py` と同じく 192×120 の灰色の格子に縮め、格子の 0.2% より多くが 24 階調より大きく変わったフレームを変化とする。文書の部分（窓の左右 5%〜95%・上下 25%〜95%、pptx はスライドの中央 32%〜72%・30%〜78%）の最後の変化を終わりとする。窓が 8 秒変わらなくなったら録画をやめる。
- どのアプリも、ファイルなしで起動して 6 秒待ってから開かせる（アプリの起動の時間は入らない）。LibreOffice・ONLYOFFICE は、コマンドラインで起動中のアプリにファイルを渡す。ibisOffice は Linux ではコマンドラインで渡したファイルを新しいプロセスで開くので、起動の前に `~/.local/share/ibisOffice/recent-files.txt` の 1 行目にそのファイルを書いておき、開始画面の「最近開いたファイル」の一番上の項目を XTest でクリックした時点から計る。
- 計測中に別の窓（ターミナルなど）がアプリの窓の前に出た回は、自動でやり直す（`bench.py` の `covering`）。
- CSV：LibreOffice は `--infilter=CSV:44,34,76,1`（テキストのインポートのダイアログの代わり）。ONLYOFFICE は「CSV オプションを選択」のダイアログの OK ボタンが出たら Enter を送り、ダイアログが出ていた時間を除く。First View は、文書の部分が最後の画面とほぼ同じ（格子の 8 割以上が同じ）になった最初のフレーム（ibisOffice は最初の画面のあとも裏で読み込み、読み込みが終わると行番号の列が少し広がるため）。上限は 900 秒。ONLYOFFICE は 100 万行を開けない（約 66 秒で「メモリ不足などの理由でファイルにアクセスできなくなりました」）。Mac での計測では 30 万行の 3.33 倍を推定値としたが、Linux では 30 万行も同じエラーで開けないので、10 万行で計り、その 10 倍（100 万 ÷ 10 万）を推定値とする（`report.py` の `CSV_FACTOR`）。
- ONLYOFFICE は読み込み中に「…の読み込み中」の箱（窓の中央の暗い箱）を出し、その間ほとんど変わらないので、箱が出ている間は録画をやめない（`bench.py` の `OO_LOADING`）。
- PDF：ibisOffice は `--export pdf`、LibreOffice は `soffice --headless --convert-to pdf`、ONLYOFFICE は `x2t`（`scripts/office-to-pdf.sh` と同じ指示の XML）。コマンドの開始から終了まで。
