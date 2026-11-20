# icons/doc

The icons of the files ibisOffice opens (K0048): `document` (blue), `spreadsheet` (green) and `presentation` (orange), the colours of the start screen's new-file badges.
Made by `scripts/gen-doc-icons.py` (drawn in code: a page with a folded corner, a picture of what it holds and a band in the colour). The art is ibis inc.'s own (AGPL-3.0-only
with the rest of the project): nothing of Microsoft's icons, letter marks or any other company's logo is copied.

- `<kind>/<size>.png` (16 to 1024): Linux's hicolor `mimetypes` (`scripts/bundle-linux.sh`).
- `<kind>.icns`: macOS, the document types of `Info.plist` (`scripts/bundle-macos.sh`: `DocIcon`, `SheetIcon`, `SlideIcon`).
- `<kind>.ico`: Windows, embedded in ibisOffice.exe as icon groups 2, 3 and 4 (`build.rs`), named by the program IDs `open_with.rs` registers.
