#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
# Build the browser version into www/.
#
#   scripts/build-web.sh            debug build
#   scripts/build-web.sh --release  release build (plus wasm-opt when installed)
#
# Needs: rustup, wasm-bindgen-cli whose version matches the wasm-bindgen crate
# in Cargo.lock, and `npm ci` for tsc. The Rust toolchain is chosen here: the
# wasm_thread crate (threads in Web Workers) uses #![feature], so the browser
# version builds with a pinned nightly (installed with the wasm32 target when
# missing); RUSTUP_TOOLCHAIN set by the caller wins.
set -euo pipefail
cd "$(dirname "$0")/.."
# The distribution keeps the release defaults (no incremental, default LTO); Cargo.toml's release profile is for development.
source scripts/dist-profile.sh

WEB_TOOLCHAIN=nightly-2026-09-17
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-$WEB_TOOLCHAIN}"
if ! rustup run "$RUSTUP_TOOLCHAIN" rustc --version >/dev/null 2>&1; then
  rustup toolchain install "$RUSTUP_TOOLCHAIN" --profile minimal --target wasm32-unknown-unknown
elif ! rustup target list --installed --toolchain "$RUSTUP_TOOLCHAIN" | grep -qx wasm32-unknown-unknown; then
  rustup target add wasm32-unknown-unknown --toolchain "$RUSTUP_TOOLCHAIN"
fi

PROFILE=debug
FLAGS=()
if [[ "${1:-}" == "--release" ]]; then PROFILE=release; FLAGS=(--release); fi

# A deep UI tree needs more than wasm-ld's default 1 MiB stack in debug builds
# (gpui-component's own web gallery does the same).
# The paths of this machine are not to go into the wasm (see bundle-macos.sh):
# rewritten, in a target directory of its own.
REMAP="--remap-path-prefix=$HOME/.cargo/registry/src=crates.io --remap-path-prefix=$HOME/.cargo/git/checkouts=git --remap-path-prefix=$HOME/.rustup=rustup --remap-path-prefix=$(pwd)=."
RUSTFLAGS="$REMAP" CARGO_TARGET_DIR=target/dist cargo rustc -p ibis-office-app --lib --crate-type cdylib --target wasm32-unknown-unknown ${FLAGS[@]+"${FLAGS[@]}"} -- \
  -C link-arg=-zstack-size=8388608

WASM="target/dist/wasm32-unknown-unknown/$PROFILE/ibis_office_app.wasm"
wasm-bindgen "$WASM" --out-dir www/pkg --target web

if [[ "$PROFILE" == release ]] && command -v wasm-opt >/dev/null; then
  wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
    www/pkg/ibis_office_app_bg.wasm -o www/pkg/ibis_office_app_bg.wasm
fi

# The wasm's size, for the download's progress bar: a server that compresses
# it (GitHub Pages sends gzip) gives Content-Length of the compressed bytes,
# while the browser hands main.ts the uncompressed ones.
stat -f %z www/pkg/ibis_office_app_bg.wasm > www/pkg/ibis_office_app_bg.wasm.size

# main.ts imports the typings wasm-bindgen just wrote.
if [[ ! -x node_modules/.bin/tsc ]]; then
  echo "TypeScript is not installed; run: npm ci" >&2
  exit 1
fi
node_modules/.bin/tsc -p www

# Icons are fetched on demand from /assets/icons/*.svg.
ASSETS_DIR=$(cargo metadata --format-version 1 | python3 -c '
import json, os, sys
meta = json.load(sys.stdin)
pkg = next(p for p in meta["packages"] if p["name"] == "gpui-kit-assets")
print(os.path.join(os.path.dirname(pkg["manifest_path"]), "assets"))')
mkdir -p www/assets
rm -rf www/assets/icons
cp -R "$ASSETS_DIR/icons" www/assets/icons

# The art page borders' pictures, ECMA-376's PNG files taken out of the zip
# unmodified (fetched one by one when a document uses them), with the notice.
rm -rf www/assets/artborders
mkdir -p www/assets/artborders
unzip -q -o crates/ibis-pptx/data/artborders/OfficeOpenXML-WordprocessingMLArtBorders.zip -d www/assets/artborders
cp crates/ibis-pptx/data/artborders/ECMA-NOTICE.txt www/assets/artborders/

echo "Built www/ ($PROFILE). Serve it, e.g.: python3 -m http.server -d www 8080"
