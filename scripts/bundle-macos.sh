#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
# Build the desktop version as a macOS app bundle:
#
#   scripts/bundle-macos.sh            debug build   (target/debug/ibisOffice.app)
#   scripts/bundle-macos.sh --release  release build (target/release/ibisOffice.app)
#   scripts/bundle-macos.sh --quick    the everyday release build (target/release/ibisOffice.app) for this Mac only:
#                                      the development profile (incremental) in target/release, so an up-to-date
#                                      target/release/ibisOffice is packed as it is (seconds) and a stale one is brought up
#                                      to date first. Its executable keeps this Mac's paths: never distribute it.
#
# A bare executable opened from Finder starts in Terminal; an .app bundle does not.
# UI icons and fonts are embedded in the executable; the bundle adds the app icon
# (crates/ibis-office-app/macos/AppIcon.icns, made by scripts/gen-macos-icon.sh), the
# third-party licences (Contents/Resources/Licenses), and the automatic update's
# ibisOfficeUpdater (Contents/MacOS).
set -euo pipefail
cd "$(dirname "$0")/.."
QUICK=
[[ "${1:-}" == "--quick" ]] && QUICK=1
# The distribution keeps the release defaults (no incremental, default LTO); Cargo.toml's release profile is for development.
source scripts/dist-profile.sh
# --quick builds the everyday target/release, with the development profile (Cargo.toml) and the planned menu lines shown.
[[ -z $QUICK ]] || unset CARGO_PROFILE_RELEASE_INCREMENTAL CARGO_PROFILE_RELEASE_LTO IBIS_OFFICE_DIST

PROFILE=debug
FLAGS=()
if [[ "${1:-}" == "--release" || -n $QUICK ]]; then PROFILE=release; FLAGS=(--release); fi

# The paths of this machine (the home folder, the repository) are not to go
# into the executable (Rust keeps source paths for panics and the like): they
# are rewritten to short relative ones, in a target directory of its own so
# that the everyday builds (target/debug, target/release) are not rebuilt by the flags.
REMAP="--remap-path-prefix=$HOME/.cargo/registry/src=crates.io --remap-path-prefix=$HOME/.cargo/git/checkouts=git --remap-path-prefix=$HOME/.rustup=rustup --remap-path-prefix=$(pwd)=."
if [[ -n $QUICK ]]; then
  cargo build --release --locked -p ibis-office-app -p ibis-office-updater
  BIN_DIR=target/release
elif [[ "$PROFILE" == release ]]; then
  RUSTFLAGS="$REMAP" CARGO_TARGET_DIR=target/dist cargo build --release --locked -p ibis-office-app -p ibis-office-updater
  BIN_DIR=target/dist/release
else
  cargo build --locked -p ibis-office-app -p ibis-office-updater
  BIN_DIR=target/debug
fi

# The version shown for macOS, from version.txt (a platform, a tab and its
# version, per line).
VERSION=$(awk -F'\t' '$1 == "macOS" { sub(/\r$/, "", $2); print $2 }' version.txt)
[[ -n $VERSION ]] || { echo "version.txt has no macOS line" >&2; exit 1; }

APP=target/$PROFILE/ibisOffice.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN_DIR/ibisOffice" "$APP/Contents/MacOS/ibisOffice"
# The automatic update (todo-fix #35): ibisOffice starts the ibisOfficeUpdater of
# the NEW app it downloaded, which swaps the apps. It is inside the signed (and
# notarized) app, so it cannot be swapped for another.
cp "$BIN_DIR/ibisOfficeUpdater" "$APP/Contents/MacOS/ibisOfficeUpdater"
cp crates/ibis-office-app/macos/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
# The third-party components in the app and the notices their licences ask
# for (made afresh from the dependencies), and Carlito's licence on its own
# (OFL-1.1 asks for it with the font).
python3 scripts/gen-web-licenses.py macos
mkdir -p "$APP/Contents/Resources/Licenses"
cp crates/ibis-office-app/macos/license.md "$APP/Contents/Resources/Licenses/license.md"
cp crates/ibis-office-render/fonts/carlito/OFL.txt "$APP/Contents/Resources/Licenses/Carlito-OFL.txt"
# The pictures of Word's art page borders: ECMA-376's zip, unmodified, with
# its notice (read when a document uses an art border; not in the executable).
mkdir -p "$APP/Contents/Resources/artborders"
cp crates/ibis-pptx/data/artborders/OfficeOpenXML-WordprocessingMLArtBorders.zip crates/ibis-pptx/data/artborders/ECMA-NOTICE.txt "$APP/Contents/Resources/artborders/"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>ibisOffice</string>
	<key>CFBundleDisplayName</key>
	<string>ibisOffice</string>
	<key>CFBundleIdentifier</key>
	<string>jp.ne.ibis.office</string>
	<key>CFBundleIconFile</key>
	<string>AppIcon</string>
	<key>CFBundleExecutable</key>
	<string>ibisOffice</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleVersion</key>
	<string>$VERSION</string>
	<key>CFBundleShortVersionString</key>
	<string>$VERSION</string>
	<key>LSMinimumSystemVersion</key>
	<string>11.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSAppleEventsUsageDescription</key>
	<string>ファイル &gt; 印刷… で、プレビューの印刷のダイアログを開くために使います。 / Used to open Preview's print dialog for File &gt; Print.</string>
	<key>CFBundleDocumentTypes</key>
	<array>
		<dict>
			<key>CFBundleTypeName</key>
			<string>Office document</string>
			<key>CFBundleTypeRole</key>
			<string>Editor</string>
			<key>LSHandlerRank</key>
			<string>Alternate</string>
			<key>LSItemContentTypes</key>
			<array>
				<string>org.openxmlformats.presentationml.presentation</string>
				<string>org.openxmlformats.spreadsheetml.sheet</string>
				<string>org.openxmlformats.wordprocessingml.document</string>
			</array>
		</dict>
		<dict>
			<key>CFBundleTypeName</key>
			<string>CSV / TSV text table</string>
			<key>CFBundleTypeRole</key>
			<string>Editor</string>
			<key>LSHandlerRank</key>
			<string>Alternate</string>
			<key>LSItemContentTypes</key>
			<array>
				<string>public.comma-separated-values-text</string>
				<string>public.tab-separated-values-text</string>
			</array>
			<key>CFBundleTypeExtensions</key>
			<array>
				<string>csv</string>
				<string>tsv</string>
				<string>tab</string>
			</array>
		</dict>
		<dict>
			<key>CFBundleTypeName</key>
			<string>Old Office and OpenDocument file (Word 97-2003, PowerPoint 97-2003, Excel 97-2003, OpenDocument text, spreadsheet and presentation)</string>
			<key>CFBundleTypeRole</key>
			<string>Editor</string>
			<key>LSHandlerRank</key>
			<string>Alternate</string>
			<key>LSItemContentTypes</key>
			<array>
				<string>com.microsoft.word.doc</string>
				<string>com.microsoft.word.dot</string>
				<string>com.microsoft.powerpoint.ppt</string>
				<string>com.microsoft.powerpoint.pps</string>
				<string>com.microsoft.powerpoint.pot</string>
				<string>com.microsoft.excel.xls</string>
				<string>com.microsoft.excel.xlt</string>
				<string>org.oasis-open.opendocument.text</string>
				<string>org.oasis-open.opendocument.text-template</string>
				<string>org.oasis-open.opendocument.spreadsheet</string>
				<string>org.oasis-open.opendocument.spreadsheet-template</string>
				<string>org.oasis-open.opendocument.presentation</string>
				<string>org.oasis-open.opendocument.presentation-template</string>
			</array>
			<key>CFBundleTypeExtensions</key>
			<array>
				<string>doc</string>
				<string>dot</string>
				<string>ppt</string>
				<string>pps</string>
				<string>pot</string>
				<string>xls</string>
				<string>xlt</string>
				<string>odt</string>
				<string>ott</string>
				<string>ods</string>
				<string>ots</string>
				<string>odp</string>
				<string>otp</string>
			</array>
		</dict>
		<dict>
			<key>CFBundleTypeName</key>
			<string>Office document with macros (the macros do not run)</string>
			<key>CFBundleTypeRole</key>
			<string>Editor</string>
			<key>LSHandlerRank</key>
			<string>Alternate</string>
			<key>CFBundleTypeExtensions</key>
			<array>
				<string>docm</string>
				<string>xlsm</string>
				<string>pptm</string>
			</array>
		</dict>
	</array>
</dict>
</plist>
EOF

# Ad-hoc signature so Apple Silicon runs the copied executables (the updater is
# signed on its own first, then the app seals it).
codesign --force --sign - "$APP/Contents/MacOS/ibisOfficeUpdater"
codesign --force --sign - "$APP"

# Tell Finder and the Dock the app changed, so they show the current icon
# instead of a cached (or blank) one after a rebuild in the same place.
touch "$APP"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP" || true

echo "Built $APP. Open it from Finder or with: open $APP"
