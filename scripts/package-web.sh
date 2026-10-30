#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
# Pack the browser version (www/, built by scripts/build-web.sh --release) for
# the release's download, the same files as the playground's app/ (see
# scripts/playground.sh), into target/dist/:
#
#   ibisOffice-<version>-Web.zip              index.html, main.js, license.md, pkg/, assets/
#   ibisOffice-<version>-Web.zip.sha256       SHA-256 of the zip ("shasum -a 256" format)
#   ibisOffice-<version>-Web.files.sha256     SHA-256 of every file in the zip, by path
#
# Whoever embeds the browser version checks what it fetched with these
# (publish/doc/web-embedding.md). The zip is reproducible: files in sorted
# order, a fixed modification time, no extra fields, so the same www/ gives the
# same zip. The paths in the zip are relative to its root (it is the app/ folder).
#
#   scripts/package-web.sh <version>      e.g. scripts/package-web.sh 0.8.5
#
# <version> must be the wasm line of version.txt. Build www/ first:
#   scripts/build-web.sh --release
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=${1:-}
[[ -n $VERSION ]] || { echo "usage: scripts/package-web.sh <version>" >&2; exit 1; }
WANT=$(awk -F'\t' '$1 == "wasm" { sub(/\r$/, "", $2); print $2 }' version.txt)
[[ $VERSION == "$WANT" ]] || {
  echo "version $VERSION is not the wasm line of version.txt ($WANT); fix version.txt or the argument" >&2
  exit 1
}

for f in www/index.html www/main.js www/license.md www/pkg/ibis_office_app.js www/pkg/ibis_office_app_bg.wasm \
         www/assets/icons www/assets/artborders/ECMA-NOTICE.txt; do
  [[ -e $f ]] || { echo "$f is missing; build www/ first: scripts/build-web.sh --release" >&2; exit 1; }
done

OUT=target/dist
NAME=ibisOffice-$VERSION-Web
mkdir -p "$OUT"
rm -f "$OUT/$NAME.zip" "$OUT/$NAME.zip.sha256" "$OUT/$NAME.files.sha256"

python3 -I - "$OUT" "$NAME" <<'PY'
import hashlib, os, sys, zipfile

out, name = sys.argv[1:3]
# What the playground's app/ holds (scripts/playground.sh): no typings (*.d.ts), no .DS_Store.
files = []
for top in ("index.html", "main.js", "license.md"):
    files.append(os.path.join("www", top))
for top in ("pkg", "assets"):
    for root, _dirs, names in os.walk(os.path.join("www", top)):
        for n in names:
            if n == ".DS_Store" or (top == "pkg" and n.endswith(".d.ts")):
                continue
            files.append(os.path.join(root, n))
rels = sorted(os.path.relpath(f, "www").replace(os.sep, "/") for f in files)

lines = []
with zipfile.ZipFile(os.path.join(out, name + ".zip"), "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for rel in rels:
        data = open(os.path.join("www", rel), "rb").read()
        info = zipfile.ZipInfo(rel, date_time=(2020, 1, 1, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        info.external_attr = 0o100644 << 16
        info.create_system = 3
        z.writestr(info, data, compresslevel=9)
        lines.append("%s  %s\n" % (hashlib.sha256(data).hexdigest(), rel))
open(os.path.join(out, name + ".files.sha256"), "w", newline="\n").write("".join(lines))
print("packed %d files" % len(rels))
PY

(cd "$OUT" && shasum -a 256 "$NAME.zip" > "$NAME.zip.sha256")
echo "$OUT/$NAME.zip ($(du -h "$OUT/$NAME.zip" | cut -f1))"
cat "$OUT/$NAME.zip.sha256"
echo "$OUT/$NAME.files.sha256 ($(wc -l < "$OUT/$NAME.files.sha256" | tr -d ' ') files)"
