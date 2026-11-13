#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
"""Write the lists of third-party components shipped and the notices their
licences require: www/license.md for the browser version,
crates/ibis-office-app/macos/license.md for the desktop app (copied into
ibisOffice.app by scripts/bundle-macos.sh), and
crates/ibis-office-app/linux/license.md for the Linux AppImage (copied into
it by scripts/bundle-linux.sh).

    scripts/gen-web-licenses.py          # all
    scripts/gen-web-licenses.py web      # www/license.md only
    scripts/gen-web-licenses.py macos    # the macOS app's only
    scripts/gen-web-licenses.py linux    # the Linux AppImage's only

Rust crates are those linked into each build (cargo tree for
wasm32-unknown-unknown, aarch64-apple-darwin and x86_64-unknown-linux-gnu,
with the AppImage's runtime for Linux); their copyright lines are taken from
each crate's own licence files. Run it again after changing dependencies.
"""
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# Output, cargo target and name of each build.
BUILDS = {
    "web": (os.path.join(ROOT, "www", "license.md"), "wasm32-unknown-unknown", "Web 版"),
    "macos": (os.path.join(ROOT, "crates", "ibis-office-app", "macos", "license.md"), "aarch64-apple-darwin", "macOS 版"),
    "linux": (os.path.join(ROOT, "crates", "ibis-office-app", "linux", "license.md"), "x86_64-unknown-linux-gnu", "Linux 版"),
}
# What each build ships besides the app: the AppImage's runtime is in the Linux one.
EXTRA_PACKAGES = {"linux": ["ibis-appimage"]}
OWN = {"ibis-office-app", "ibis-office-shell", "ibis-office-pptx", "ibis-office-docx", "ibis-office-xlsx", "ibis-office-render", "ibis-office-base", "ibis-doc-model", "ibis-pptx", "ibis-emf", "ibis-eot", "ibis-imgdiff", "ibis-appimage"}


def run(*args):
    return subprocess.run(args, cwd=ROOT, check=True, capture_output=True, text=True).stdout


def crates(target, extra=()):
    tree = ""
    for package in ["ibis-office-app", *extra]:
        tree += run("cargo", "tree", "--locked", "-p", package, "--target", target,
                    "-e", "normal", "--prefix", "none", "-f", "{p}")
    names = set()
    for line in tree.splitlines():
        m = re.match(r"(\S+) v(\S+)", line.strip())
        if m:
            names.add((m.group(1), m.group(2)))
    meta = json.loads(run("cargo", "metadata", "--locked", "--format-version", "1"))
    by_id = {(p["name"], p["version"]): p for p in meta["packages"]}
    out = []
    for name, version in sorted(names):
        if name in OWN:
            continue
        p = by_id.get((name, version))
        if p is None:
            continue
        out.append(p)
    return out


def copyright_lines(package):
    """Copyright lines from a crate's licence and notice files."""
    folder = os.path.dirname(package["manifest_path"])
    lines = []
    for f in sorted(os.listdir(folder)):
        if not re.match(r"(?i)(licen[cs]e|copying|notice|copyright)", f):
            continue
        try:
            text = open(os.path.join(folder, f), encoding="utf-8", errors="replace").read()
        except (IsADirectoryError, OSError):
            continue
        for line in text.splitlines():
            line = line.strip()
            # Real notices, not the template lines of licence texts.
            if re.match(r"(?i)^(copyright|\(c\)|©)", line) and "[yyyy]" not in line and "<year>" not in line \
                    and "{yyyy}" not in line and "[year]" not in line and len(line) < 200:
                if line not in lines:
                    lines.append(line)
    return lines


LICENSE_TEXTS = {
    "MIT": """Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.""",
}


def find_license_file(package, pattern):
    folder = os.path.dirname(package["manifest_path"])
    for f in sorted(os.listdir(folder)):
        if re.match(pattern, f, re.I):
            return open(os.path.join(folder, f), encoding="utf-8", errors="replace").read().strip()
    return None


def main():
    builds = sys.argv[1:] or list(BUILDS)
    for build in builds:
        if build not in BUILDS:
            sys.exit(f"unknown build {build}: one of {', '.join(BUILDS)}")
        write(build)


def write(build):
    out_path, target, label = BUILDS[build]
    packages = crates(target, EXTRA_PACKAGES.get(build, ()))
    licences = {}
    rows = []
    notices = []
    for p in packages:
        lic = p.get("license") or "（license-file：" + str(p.get("license_file")) + "）"
        repo = p.get("repository") or p.get("homepage") or f"https://crates.io/crates/{p['name']}"
        rows.append(f"| {p['name']} | {p['version']} | {lic} | {repo} |")
        for ident in re.findall(r"[A-Za-z0-9.\-]+(?: WITH [A-Za-z0-9.\-]+)?", lic.replace("/", " OR ")):
            if ident not in ("OR", "AND", "WITH"):
                licences.setdefault(ident, p)
        lines = copyright_lines(p)
        if lines:
            notices.append((f"{p['name']} {p['version']}", lines))

    # Full texts of the licences, from a crate that ships a file with that
    # text (recognised by its wording).
    markers = {
        "Apache-2.0": ["Apache License", "Version 2.0"],
        "BSD-2-Clause": ["Redistribution and use", "2. Redistributions in binary form"],
        "BSD-3-Clause": ["Redistribution and use", "Neither the name"],
        "ISC": ["Permission to use, copy, modify, and/or distribute"],
        "Zlib": ["This software is provided 'as-is'", "altered source versions"],
        "Unicode-3.0": ["UNICODE LICENSE V3"],
        "MPL-2.0": ["Mozilla Public License Version 2.0"],
        "BSL-1.0": ["Boost Software License - Version 1.0"],
        "0BSD": ["Permission to use, copy, modify, and/or distribute", "WITHOUT ANY WARRANTY"],
        "Unlicense": ["This is free and unencumbered software"],
        "bzip2-1.0.6": ["bzip2", "Redistribution and use"],
        "CC0-1.0": ["CC0 1.0 Universal"],
    }
    users = {}
    for p in packages:
        lic = (p.get("license") or "").replace("/", " OR ")
        for ident in re.findall(r"[A-Za-z0-9.\-]+", lic):
            users.setdefault(ident, []).append(p)
    texts = []
    for ident in sorted(licences):
        if ident in LICENSE_TEXTS:
            texts.append((ident, LICENSE_TEXTS[ident]))
            continue
        want = markers.get(ident)
        if not want:
            continue
        found = None
        for p in users.get(ident, []):
            folder = os.path.dirname(p["manifest_path"])
            for f in sorted(os.listdir(folder)):
                if not re.match(r"(?i)(licen[cs]e|copying)", f):
                    continue
                path = os.path.join(folder, f)
                if not os.path.isfile(path):
                    continue
                t = open(path, encoding="utf-8", errors="replace").read().strip()
                if all(m in t for m in want) and len(t) < 30_000:
                    found = t
                    break
            if found:
                break
        if found:
            texts.append((ident, found))
        else:
            # Only offered as an alternative (such as "Unlicense OR MIT"), or its
            # crates are listed with their own text below.
            pass

    # A crate under a single licence other than MIT and Apache ships its own
    # text (with its notice); include it whole.
    own_texts = []
    for p in packages:
        lic = (p.get("license") or "").strip()
        if not lic or re.search(r"\b(OR|AND)\b|/", lic) or lic in ("MIT", "Apache-2.0"):
            continue
        folder = os.path.dirname(p["manifest_path"])
        for f in sorted(os.listdir(folder)):
            path = os.path.join(folder, f)
            if re.match(r"(?i)(licen[cs]e|copying)", f) and os.path.isfile(path):
                own_texts.append((f"{p['name']} {p['version']}（{lic}）", open(path, encoding="utf-8", errors="replace").read().strip()))
                break

    ecma = open(os.path.join(ROOT, "crates", "ibis-pptx", "data", "ECMA-NOTICE.txt"), encoding="utf-8").read().strip()
    # The art page borders' pictures: the desktop app reads them from the zip
    # in its Resources; the web version serves its PNG files one by one.
    ecma_art = open(os.path.join(ROOT, "crates", "ibis-pptx", "data", "artborders", "ECMA-NOTICE.txt"), encoding="utf-8").read().strip()
    plex = open(os.path.join(ROOT, "crates", "ibis-office-app", "fonts", "OFL.txt"), encoding="utf-8").read().strip()
    carlito = open(os.path.join(ROOT, "crates", "ibis-office-render", "fonts", "carlito", "OFL.txt"), encoding="utf-8").read().strip()
    assets = json.loads(run("cargo", "metadata", "--locked", "--format-version", "1"))
    lucide_dir = next(os.path.dirname(p["manifest_path"]) for p in assets["packages"] if p["name"] == "gpui-kit-assets")
    lucide = open(os.path.join(lucide_dir, "LICENSE-LUCIDE"), encoding="utf-8").read().strip()

    # The data, fonts and icons bundled: IBM Plex Sans is the web version's
    # UI font; Carlito stands in for Calibri in the desktop app.
    bundled = [("ECMA-376 presetShapeDefinitions.xml", "図形（プリセット図形）の輪郭の定義", "Ecma テキスト著作権ポリシー"),
               ("ECMA-376 presetTableStyles.xml", "Excel の組み込みテーブル スタイルの定義", "Ecma テキスト著作権ポリシー"),
               ("ECMA-376 presetCellStyles.xml", "Excel の組み込みセルのスタイルの定義", "Ecma テキスト著作権ポリシー")]
    bundled.append(("ECMA-376 OfficeOpenXML-WordprocessingMLArtBorders.zip", "Word のページの絵柄罫線の絵" + ("（`assets/artborders/` に PNG を 1 枚ずつ、改変せずに）" if build == "web" else ""), "Ecma テキスト著作権ポリシー"))
    if build == "web":
        bundled.append(("IBM Plex Sans", "UI の文字（Web 版）", "SIL Open Font License 1.1"))
    else:
        bundled.append(("Carlito", "Calibri が入っていないときの代わりの文字", "SIL Open Font License 1.1"))
    bundled.append(("Lucide", "アイコン（`assets/icons/`）" if build == "web" else "アイコン（実行ファイルに埋め込み）", "ISC"))
    # Material Symbols: the chrome's icons (crates/ibis-office-shell/icons/material/),
    # embedded in the binary on both builds.
    bundled.append(("Material Symbols（Google）", "タイトル行・メニュー・ツールバー・サイドバーのアイコン（実行ファイルに埋め込み）", "Apache-2.0"))
    material_readme = open(os.path.join(ROOT, "crates", "ibis-office-shell", "icons", "material", "README.md"), encoding="utf-8").read()
    material_source = next((l[2:] for l in material_readme.splitlines() if l.startswith("- 出典")), "")
    where = {"web": " Web 版（`www/`）", "macos": " macOS 版（`ibisOffice.app`）", "linux": " Linux 版（AppImage）"}[build]
    md = [f"# サードパーティのライセンス（{label}）",
          "",
          f"ibisOffice の{where}に含まれる、ibisOffice 以外の部品と、そのライセンスが求める表示です。"
          "このファイルは `scripts/gen-web-licenses.py` で生成しています。",
          "",
          "## 同梱しているデータ・フォント・アイコン",
          "",
          "| 部品 | 用途 | ライセンス |",
          "|---|---|---|",
          *[f"| {name} | {use} | {lic} |" for name, use, lic in bundled],
          "",
          f"## Rust の部品（{len(rows)} 件）",
          "",
          "| 名前 | 版 | ライセンス | 入手先 |",
          "|---|---|---|---|",
          *rows,
          "",
          "MPL-2.0 の部品は、ソースを改変せずに使っています。ソースは上の入手先と crates.io から入手できます。",
          "",
          "## 著作権表示",
          "",
          "### ECMA-376（Ecma International）",
          "",
          "```text", ecma, "```",
          "",
          "```text", ecma_art, "```",
          "",
          *(["### IBM Plex Sans", "", "```text", plex, "```", ""] if build == "web" else
            ["### Carlito", "", "```text", carlito, "```", ""]),
          "### Lucide",
          "",
          "```text", lucide, "```",
          "",
          "### Material Symbols（Google）",
          "",
          "Copyright Google LLC. Apache License 2.0（本文は下の「ライセンス本文」の Apache-2.0）。" + (f" {material_source}。" if material_source else ""),
          "ファイル名から寸法の接尾辞（`_20px`）を除き、塗りつぶし版は `_fill` と名付けた。図形は変えていない。",
          "",
          "### Rust の部品",
          "",
          "各部品のライセンス ファイルにある著作権表示です（ライセンス ファイルに表示がない部品は、上の一覧のライセンスと入手先に従います）。",
          ""]
    for name, lines in notices:
        md.append(f"- **{name}**：" + " / ".join(lines))
    md += ["", "## 単一のライセンス（MIT・Apache-2.0 以外）の部品のライセンス本文", ""]
    for name, text in own_texts:
        md += [f"### {name}", "", "```text", text, "```", ""]
    md += ["", "## ライセンス本文", "", "複数のライセンスから選べる部品は、MIT または Apache-2.0 を選んでいます。", ""]
    for ident, text in texts:
        md += [f"### {ident}", "", "```text", text, "```", ""]
    with open(out_path, "w", encoding="utf-8") as f:
        f.write("\n".join(md) + "\n")
    print(f"wrote {os.path.relpath(out_path, ROOT)}: {len(rows)} crates, {len(texts)} licence texts")


if __name__ == "__main__":
    sys.exit(main())
