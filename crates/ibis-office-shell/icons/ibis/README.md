# ibisOffice で描いたアイコン

ibisOffice のために描いた SVG（著作権はアイビス。ibisOffice のソースと同じ AGPL-3.0-only）。Material Symbols（`../material/`）にない図だけを、ここに置く。

- `border-*.svg`：罫線の線の種類の見本（xlsx のツールバー > 枠線 > 線の種類）。BorderLine の 13 種類に 1 つずつ。
- `merge-center.svg`・`merge-across.svg`・`unmerge.svg`：セルを結合のメニューの項目。

アプリへの埋め込みは `build.rs` が自動で行う。名前は `ibis/<ファイル名>`（`Material("ibis/border-thin")`、メニュー項目は `.icon("ibis/border-thin")`）。
