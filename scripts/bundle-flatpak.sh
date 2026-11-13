#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
# Build the Linux version as a single-file Flatpak bundle,
# target/dist/ibisOffice-<version>-Linux-<arch>.flatpak, with its SHA-256
# beside it (.sha256).
#
#   scripts/bundle-flatpak.sh            debug build
#   scripts/bundle-flatpak.sh --release  release build
#
# Like scripts/bundle-linux.sh, only the ibisOffice binary, icons, desktop/metainfo
# files, ECMA-376 art borders data, and third-party license notices are placed in
# /app. No external shared libraries (.so) are bundled inside the Flatpak.
set -euo pipefail
cd "$(dirname "$0")/.."
# The distribution keeps the release defaults (no incremental, default LTO); Cargo.toml's release profile is for development.
source scripts/dist-profile.sh

PROFILE=debug
FLAGS=()
if [[ "${1:-}" == "--release" ]]; then PROFILE=release; FLAGS=(--release); fi

ARCH=$(uname -m)
case "$ARCH" in
  x86_64|aarch64) ;;
  arm64) ARCH=aarch64 ;;
  *) echo "unsupported architecture for Flatpak: $ARCH" >&2; exit 1 ;;
esac

REMAP="--remap-path-prefix=$HOME/.cargo/registry/src=crates.io --remap-path-prefix=$HOME/.cargo/git/checkouts=git --remap-path-prefix=$HOME/.rustup=rustup --remap-path-prefix=$(pwd)=."
RUSTFLAGS="$REMAP" CARGO_TARGET_DIR=target/dist cargo build ${FLAGS[@]+"${FLAGS[@]}"} --locked -p ibis-office-app

VERSION=$(awk -F'\t' '$1 == "Linux" { sub(/\r$/, "", $2); print $2 }' version.txt)
[[ -n $VERSION ]] || { echo "version.txt has no Linux line" >&2; exit 1; }

WORK=target/dist/flatpak
STAGE="$WORK/staged"
rm -rf "$WORK"
mkdir -p \
  "$STAGE/bin" \
  "$STAGE/share/applications" \
  "$STAGE/share/metainfo" \
  "$STAGE/share/ibisOffice/artborders" \
  "$STAGE/share/doc/ibisOffice"

strip -o "$STAGE/bin/ibisOffice" "target/dist/$PROFILE/ibisOffice"

# Desktop entry with Icon matching the Flatpak application ID (jp.ne.ibis.ibisOffice).
sed 's/^Icon=ibisoffice$/Icon=jp.ne.ibis.ibisOffice/' \
  crates/ibis-office-app/linux/ibisoffice.desktop \
  > "$STAGE/share/applications/jp.ne.ibis.ibisOffice.desktop"

cp crates/ibis-office-app/linux/jp.ne.ibis.ibisOffice.metainfo.xml \
  "$STAGE/share/metainfo/jp.ne.ibis.ibisOffice.metainfo.xml"

ICONS=crates/ibis-office-shell/logo/png/ico
for size in 16 24 32 48 256; do
  mkdir -p "$STAGE/share/icons/hicolor/${size}x${size}/apps"
  cp "$ICONS/ibisOffice_icon_win_${size}px_transparent.png" \
    "$STAGE/share/icons/hicolor/${size}x${size}/apps/jp.ne.ibis.ibisOffice.png"
done

cp crates/ibis-pptx/data/artborders/OfficeOpenXML-WordprocessingMLArtBorders.zip \
  crates/ibis-pptx/data/artborders/ECMA-NOTICE.txt \
  "$STAGE/share/ibisOffice/artborders/"

python3 scripts/gen-web-licenses.py linux
DOC="$STAGE/share/doc/ibisOffice"
# The licence files are in publish/ here, at the top in the public repository (GitHub).
LIC=publish; [[ -f $LIC/LICENSE ]] || LIC=.
cp "$LIC/LICENSE" "$DOC/LICENSE"
cp "$LIC/COMMERCIAL-LICENSE.md" "$DOC/COMMERCIAL-LICENSE.md"
[[ -f $LIC/NOTICE ]] && cp "$LIC/NOTICE" "$DOC/NOTICE"
cp crates/ibis-office-app/linux/license.md "$DOC/third-party-licenses.md"
cp crates/ibis-office-render/fonts/carlito/OFL.txt "$DOC/Carlito-OFL.txt"

cp crates/ibis-office-app/linux/jp.ne.ibis.ibisOffice.yml "$WORK/jp.ne.ibis.ibisOffice.yml"

(
  cd "$WORK"
  flatpak-builder --force-clean --repo=repo build-dir jp.ne.ibis.ibisOffice.yml
)

# Verify that no shared libraries (.so) were bundled into /app.
so_files=$(find "$WORK/build-dir/files" -name '*.so' -o -name '*.so.*' || true)
if [[ -n "$so_files" ]]; then
  echo "STOP: unexpected shared libraries found in Flatpak /app:" >&2
  echo "$so_files" >&2
  exit 1
fi

OUT="target/dist/ibisOffice-$VERSION-Linux-$ARCH.flatpak"
rm -f "$OUT"
flatpak build-bundle "$WORK/repo" "$OUT" jp.ne.ibis.ibisOffice \
  --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo

(cd target/dist && sha256sum "$(basename "$OUT")" > "$(basename "$OUT").sha256")
ls -l "$OUT"
cat "$OUT.sha256"
