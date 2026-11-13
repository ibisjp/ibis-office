# Changelog

All notable changes to ibisOffice are recorded in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.8.7] - 2026-11-13

The two large changes of this version:

- **The menu bar is redesigned** in the three screens, Document, Spreadsheet and Presentation: commands that left a menu are reached by the command search, the toolbar, the right-click menus or the side bars, and the Window menu, the Start window line, access keys and keyboard-ready message boxes come with it.
- **The source is split into crates**: a shared layer (`ibis-office-shell`) and one crate per format (`ibis-office-docx`, `ibis-office-xlsx`, `ibis-office-pptx`) on top of it, so the formats build and change apart (see "For developers and forks" below).

### Added

#### All apps

- Setup dialog (Help > Setup) that downloads fonts: it lists the fonts that are not installed with their size and license (a link to each font's license file), with Select All, Install, Don't Install and Cancel; it shows the progress and checks each download's size and SHA-256. The first font offered is IPAmj Mincho (for kanji with variation selectors, #158), downloaded from GitHub Pages unmodified and kept apart from the app. The dialog opens at the first start and after an update unless you chose Don't Install. Installed fonts appear in the font lists and in open documents at once, without a restart; the Web version keeps them in the browser.
- One shared Format Shape pane in the three screens, with tabs for the basic settings, decoration (shadow, glow, soft edge, reflection) and the text's own fill, outline, shadow and glow. Format Shape... is the first line of the right-click menu of a shape, picture or text box in the three screens, also on the band just outside a selected shape's frame where the cursor is a hand.
- File > Quit in the three screens on macOS, Windows and Linux (the app menu's Quit and ⌘Q stay on macOS; the Web version has none).
- Window menu in the three screens: Window List (a quick switcher in most-recently-used order, Ctrl+Tab), Next Window, Previous Window, the arrange commands, the list of open documents, and Sidebars (every pane; a pane that is not available is grey with the reason). Each window is marked with its screen's icon. The Web version, which has one window, keeps Sidebars and Full Screen in the View menu.
- The Start window is the first line of the File menu in the three screens; a recent file that cannot be opened shows an alert, a missing file's name is grey, and the version is shown.
- The menu bar folds the menus that do not fit into a down-triangle button after the last visible menu. Colored contextual menus appear for the selection.
- Menu lines have access keys on Windows and Linux: the letter is underlined, and pressing it in an open menu runs the line or opens its submenu (Alt, Alt+J and F10 reach the menu bar). Alt+letter presses the button of a message box or a dialog's action bar (#118).
- Every message box can be used from the keyboard: Tab and Shift+Tab move the blue focus ring over the buttons, Enter and Space press the focused button, and Esc presses Cancel (or No). The focus starts on the main button, never on a destructive one. When another app changed the open file, Enter reloads it and Esc answers No.
- Click the down-triangle after the file name in the title bar for Copy File Name, Copy File Path and Rename File... (the file is renamed on disk at once).
- A new file's Save As dialog opens in the OS documents folder, then in the last folder saved to (#143). Save As on Windows offers the file types (.docx, .dotx, .odt; .xlsx, .xltx, .xls, .ods, .csv, .tsv; .pptx, .ppsx, .potx, .odp), as on macOS and Linux.
- Compatibility Mode information bar for old .xls and .ppt files, as for .doc (hidden in reading view).
- Files protected with Office 2007's Standard Encryption (AES-128, AES-192 or AES-256 with SHA-1) open after the password is given, in .docx, .xlsx and .pptx and on the Web; a wrong password, a damaged file and an unsupported method are told apart. Saving keeps the password and writes the stronger Agile Encryption (#190).
- An update that this copy cannot install (a Flatpak, a build from the source, a copy that macOS moved to a temporary place) says why on every start and from the start window's button (for a Flatpak, with the install command and a copy button). On Windows a warning appears when the install folder holds your own files (#136).
- Help > Add to List of Apps on Linux (AppImage) asks again about the list of apps; the first question says that without it the dock may show no icon (#147).
- Settings is a modal dialog with a splitter whose position is kept, and a reorderable quick access table with an add menu and command icons.
- The title row's Draw button opens the Ink panel in the three screens (the button stays filled while the panel is open). The Ink panel starts with an Off tool (the pointer then behaves as without ink: a click places the caret, a drag selects), and the lasso follows the eraser.
- The title row's comments button is filled while the comments pane is open, and its add-comment button is gone (Review > New Comment and the right-click menu add a comment).
- A selected shape can be dragged from the band just outside its frame, where the cursor is a hand (before, a press 1 px outside the frame did nothing).
- The input method's clause being converted is underlined thick, converted clauses thin and unconverted text dotted, in every self-drawn text input (#138).
- Find panel buttons have tooltips with their keys; the theme color menu shows six accent chips and the theme effect menu shows three samples. Segmented controls take the arrow keys, Home and End, and skip disabled segments.

#### Documents (.docx)

- Insert > Header and Footer enters header and footer editing on the page: the body is shown faint; a close button, Esc or a double click in the body leaves it; thin grey marks show the text boundary at the four corners of the body, and blue lines show the header and footer areas.
- Insert > Shapes is a grid of small pictures of the shapes, and Insert > Chart shows a picture of each chart kind drawn from sample data (the same pictures as the sheet screen's). Insert > Text Box sits above the pictures and inserts a horizontal text box.
- Edit > Asian Layout (first in the Edit menu) offers tate-chu-yoko, combined characters and fit text.
- The Format Shape pane of a text box has a Text tab (text fill, outline, shadow and glow) that acts on the selected characters or on the whole text of the box; the effects are saved and read as Microsoft Word writes them.
- Review > Changes > Reject All Changes.
- A right-click menu in the text (#156), and line numbers that apply to every section with interval and distance options (#140).
- Bullet menus offer Microsoft Word's gallery symbols (□ ◇ ★ ☆ ✔ ❖ ✧ ➔ ■ ◆ ➢ ✓), in .docx and .pptx (#142). A new bullet list has the default bullets of Japanese Microsoft Word 365 (Wingdings ●, ➢ and ✧ in turn over the nine levels), so the second level is the arrowhead (#142).
- Tracked deletion of a paragraph mark: it is read and saved, the paragraph is shown joined where deleted text is hidden, joined on Accept and kept on Reject; Backspace and Delete across a paragraph boundary with recording on mark the deletion (#123).
- Editing marks follow Microsoft Word: a Japanese UI shows ↵ for the end of a paragraph and ↓ for a line break, page and column breaks are drawn as a dashed line with a name, the tab after a bullet shows →, and Ctrl+Enter inserts a page break (#116, #117). In vertical writing the marks are drawn turned with the line (#116).
- Inside a tate-chu-yoko run in vertical text the caret is a vertical bar between the digits; Down goes to the next digit and Up goes back.
- The caret, Backspace and Delete move over a kanji and its variation selector (an IVS) as one character, in the three screens (#158).

#### Spreadsheets (.xlsx)

- The check box of Microsoft 365 is read, drawn as Microsoft Excel draws it, toggled by click and Space, inserted from Insert > Checkbox and saved (#32).
- Copy and paste in Microsoft Excel's own clipboard forms: on macOS a one-sheet workbook in Microsoft Excel's native form is read first and written next to the HTML; on Windows cells are copied as an XML Spreadsheet that Microsoft Excel pastes with formulas and formats, and Microsoft Excel's native form is read when pasting. Cells are also copied and pasted as formatted HTML.
- Paste Special takes the cells another program put on the clipboard (a Microsoft Excel worksheet or an HTML table), not only the last copy made here (#134).
- Copying cells outlines them with the dashed line, as a cut does, until Esc or typing into a cell.
- Copying a whole dynamic array's spill range pastes the formula only, and it spills again; spilled cells copied without their formula paste nothing (except Paste Values); cutting the formula cell takes the whole array and cutting spilled cells alone is refused, as in Microsoft Excel (#176).
- Phonetic guides (furigana) are shown as Microsoft Excel shows them: only for cells that have a guide, in the guide's typeface and size, aligned as the cell is, with the guide's line added to the row height (#170).
- Cells with a volatile function (TODAY, NOW, RAND and so on) and the cells that depend on them are calculated again when a workbook is opened, except in manual calculation mode (#177).
- Typing `@` and a function name followed by `(`, such as `@sum(a1:a2)`, enters the formula `=sum(a1:a2)` as Microsoft Excel does, also with the full-width mark; any other entry that starts with `@` stays text (#148).
- Settings > Spreadsheet has a font for the half-width letters and digits of the formula bar (#149).
- The trim references (`A:.B`, `A.:B`, `A.:.B`) are read as TRIMRANGE's, and TRIMRANGE of a reference gives a reference, so functions such as FORMULATEXT can take it (#146, #151).
- The source of a list validation can be a formula (OFFSET, INDIRECT, SORT and others). A source Microsoft Excel does not accept there (such as TRANSPOSE) asks first, once per file, saying Microsoft Excel will not apply the rule (#154).
- The query editor's advanced editor edits the query's M code; Apply reads it back as the steps, and code it cannot read is refused with the reason and the step. The steps pane and the side panes resize by their splitters (#173).
- N and T of an array give an array, and the base-conversion, complex-number, ROMAN, ARABIC and ERROR.TYPE functions, ADDRESS, COMPLEX, DECIMAL, REGEXEXTRACT, WORKDAY, NETWORKDAYS, PERCENTILE, QUARTILE, PERCENTRANK, FVSCHEDULE, NPV, TEXTBEFORE, TEXTAFTER, VALUETOTEXT and ARRAYTOTEXT run for each element of an array given for a scalar argument, as Microsoft Excel does (#141).
- View > Freeze Panes > Freeze Panes at Selection. A green 2 px line in the headers marks hidden rows and columns.
- Each worksheet keeps its scroll position when you show another sheet and come back, whatever command changes the sheet (#145); the scroll position and selection of each sheet are saved in the file and restored when it is opened.
- Page Setup: segment rows, a wider dialog, a custom header editor that returns to the dialog, and a Margins menu with Microsoft Excel's presets.
- Fill, outline (color, width, dash) and effects changed in the Format Shape pane on a shape, line, connector or picture are saved into the file; shapes you did not touch stay byte for byte as they were.
- Deleting a sheet asks first with Yes and No. A text box on a sheet starts at 11 pt, Microsoft Excel's default for drawings.
- Design Mode shows a check on its menu line while it is on, and a red Close Design Mode pill in the title row, like the slide master's Close Master View. Insert > Checkbox has a check box icon.
- Dragging a selection past the edge of the grid scrolls the sheet while the button is held, faster the farther the pointer is, also when the pointer stops: cells, row and column headers, the fill handle and moving a range, with frozen panes kept (#186).
- A formula is entered with its references, function names, TRUE and FALSE in upper case and sheet names as the workbook spells them (`=sum(a1:b2)` becomes `=SUM(A1:B2)`); text in quotes, defined names and table names keep their case (#185).

#### Presentations (.pptx)

- Slide transitions (#88): the options of every transition are settable and named by the side the slide comes from; transitions are read and written as Microsoft PowerPoint does; wheel, clock and dissolve are drawn; the 3D transitions (cube, box, rotate, orbit, flip, gallery, conveyor, switch, ferris wheel, fall over, page curls, peel off, curtains, wind, drape, airplane, origami and the other 2013 effects) are drawn as 3D motion fitted to Microsoft PowerPoint's own video frames.
- Animations (#89): colors, fill and outline, font changes, the lean of Light Speed and effects by letters or words are drawn as Microsoft PowerPoint draws them; the More Effects lists hold every effect Microsoft PowerPoint has (291 effects).
- Text in columns (#77): vertical text takes columns too; the text box panel has a columns spinner (1 to 16) and a column spacing spinner.
- Guides of the slide master and of each layout are read, shown on the slide, snapped to by moving and resizing, and shown with their distance from the middle while dragged (Alt for 0.01 cm); the normal view's guides are read and written as Microsoft PowerPoint 365 does, and a guide's color survives saving and opening again (#84).
- ibisOffice's own features that Microsoft PowerPoint lacks, each asked about once per file: text wrap around pictures and videos, with four distances in the Size and Position panel (#132); linked text boxes whose text flows from box to box while you type (also while an input method composes), with link and unlink, an overflow mark, arrow keys that cross from box to box, Select All over the whole chain, a dashed link line, and a solid blue frame on the box with the caret and dashed frames on the other boxes of the chain (#131).
- The notes pane opens the way the file says (Microsoft PowerPoint's normal view settings), and moving, hiding or showing it is saved; dragging the border to the bottom hides the notes pane or the slide pane.

#### Web version

- The media player plays embedded video and audio through the browser, shows the length when known, offers QuickTime movies as MP4, and says what the browser says when a format cannot be played (#125).
- File > New of an open document and the menu bar's logo open another browser tab with a new file of that kind.

#### Command line

- `--export row-heights <file> [out.tsv]` writes each sheet's row heights in points as the app lays them out when it opens the file (.xlsx, .xls).

### Changed

#### All apps

- The menu bar is reorganized in the three screens, Document, Spreadsheet and Presentation. Commands that left a menu are found by the command search, the toolbar, the right-click menus or the side bars.
- The save prompt's buttons are Save, Cancel and Don't Save in that order; Don't Save is red and Enter presses Save.
- When a typeface is missing on the PC, the stand-in follows what the file says about the typeface, not its name: in .xlsx files by the font's family, in .docx files by the font table's character set, in .pptx files by the run's East Asian character set. A Japanese PC that lists Yu Gothic or MS Mincho only by their Japanese names finds them too.
- About ibisOffice is one shared dialog on every OS. All eight table screens use one table that lifts a row at the press and reorders by dragging. Drop-downs, spinners, segmented controls, sliders, radio buttons and check boxes show a disabled and a mixed state.
- A recent file's icon is the icon of the screen that opens its extension. Opening a template from a window that already shows a document opens it in a new window.
- Saving into a sync folder such as OneDrive: when the new file beside the target cannot be made or moved over it, the file is written in place, after a copy that is put back if the writing fails (#144).
- After an update on Windows, the backup of the old version is removed the first time the new version starts.
- In a .docx with no settings part, punctuation is not compressed (Microsoft Word's default), and a document with no styles part kerns at every size (#54).

#### Documents (.docx)

- The body of a page ends above the footer that page has (default, even-page or first-page) and not above the section's tallest footer.

#### Spreadsheets (.xlsx)

- A row's height for its content is one calculation used by opening, AutoFit, editing, pasting and changing a column width; empty rows are 18.75 pt as Microsoft Excel for Windows measures, a row is measured in the face its cells are in, and a row without a hand-set height grows to fit wrapped text on entry, on turning wrap on or off, on paste and on a column width change. A medium, dashed medium, thick or double line under a row makes it one pixel taller, as in Microsoft Excel.
- Paste follows Microsoft Excel in every combination of paste type, operation, skip blanks, transpose and source (400 combinations and 198 situations of an ordinary paste checked against Microsoft Excel 365): comments, merges and validation come along as in Microsoft Excel, transposed formulas swap their reference offsets, a destination with merged ranges refuses or dissolves them, and a destination bigger than the copy is tiled by whole copies.

#### Presentations (.pptx)

- The soft edge of pictures and shapes follows Microsoft PowerPoint's measured fade: the edge is faded inward by the radius and blurred, instead of blurring the whole picture (#153).
- Apply Theme Colors to Selected Slides applies to every slide selected in the thumbnails.
- Export, copy and print are refused while the slide master view or the notes formatting is open (.ppsx and .odp no longer write a broken file), with a message that says so. The toolbar leaves Character Spacing, Change Case, Language, Format Painter and Text Effects to the menus; Insert > Text Box sits above the pictures.

#### Web version

- The Japanese text of the interface is drawn in a gothic face of the system; a font downloaded for documents is used only where a document names it or for a face of the same kind (gothic or mincho).

#### For developers and forks

- The source is split into crates: the shared layer is `crates/ibis-office-shell`, each format is its own crate (`crates/ibis-office-docx`, `crates/ibis-office-xlsx`, `crates/ibis-office-pptx`) on top of it, and `crates/ibis-office-app` keeps the start-up, the export and the tests that use several formats. The formats do not refer to each other or to the app.
  - Moved files: `crates/ibis-office-app/src/shell/` → `crates/ibis-office-shell/src/`; `src/docx/`, `src/xlsx/`, `src/pptx/` → `crates/ibis-office-<format>/src/` (each format's tests that use only that format and the shell moved with it).
  - Moved resources: the i18n tables to `crates/ibis-office-base/i18n/`; icons, logos and the Windows PNGs to `crates/ibis-office-shell/`; each format's "Open in Office" logo to its crate's `logo/`; Carlito and the bevel data to `crates/ibis-office-render/`; the art border pictures to `crates/ibis-pptx/data/`. `macos/`, `linux/`, the Windows icons and IBM Plex stay in the app.
  - A format's `impl SlideEditor` blocks are extension traits made by the attribute `#[ibis_office_macros::editor_ext(Name)]`; after adding such a block or a method, run `python3 scripts/gen-editor-ext.py` (the test `editor_ext_rule` fails otherwise), and a file that calls a format's methods from another file has `use crate::ext::*;` (in the format's crate) or `use crate::<format>::ext::*;` (in the app).
  - The shared layer cannot call a format: it asks through a hook of the format's `FormatEditor`.
  - The crates are wired at start-up by the app's `register_hooks()` (no symbols resolved by name at link time any more); a new entry point (another `main`, a test runner) calls it first, or the first use panics saying so.
  - Changes made against the old paths in a fork need to be moved to the new files; git follows the renames when merging.
- New submodules of the GPUI fork: `forks/gpui-pre-apple` (the macOS Metal layer, with offscreen rendering and textured meshes for the 3D transitions) and `forks/gpui-base`.
- The release profile has incremental compilation on for development; the distribution scripts turn incremental and LTO off.

### Fixed

- All apps: on Linux and Windows, shortcut texts show Ctrl, Alt and Shift instead of the Mac's key marks (#150).
- All apps: Save, Save As, Save a Copy, the exports and Print first enter what is being typed in a cell; before, the open entry was missing from the saved file.
- All apps: saving while a slide master is open is refused with a message before any dialog asks where to save.
- All apps: in the chart data pane, a value typed into the grid is entered with Enter and the chart changes.
- All apps: a search field of a pop-up list takes a typed space as a character (#149); the menu list's scroll bar is no longer hidden after scrolling (#175); closing a document drops its AutoRecover snapshot.
- All apps: the shadow of a picture-filled shape no longer darkens the picture; a blurred picture shadow spreads past the picture instead of leaving a hairline.
- .docx: a line on a character grid with a Yu typeface is 1.673 em high; East Asian Ambiguous characters in vertical text turn or stand as Microsoft Word draws them; a run of symbols is not broken at ※ (#54).
- .docx: a stop or closing bracket before another one is squeezed half an em as Microsoft Word does, and ＝ and the other full-width forms kept upright take the font's vertical glyph (#54, #120).
- .docx: vertical text lays out like Microsoft Word for symbols and combining marks (#54): fractions and U+23BE to U+23CD are turned, © and ® are turned half cells, combining marks of MS Mincho take a cell of their own, marks without a base at a line's start before an upright symbol share one em cell, U+302A to U+302D take no width, a soft hyphen typed as a character is drawn as a hyphen (an optional hyphen stays optional and is saved as such), and Hiragino's narrow quotation marks are centered on the column.
- .docx: a character is drawn in the font slot Microsoft Word uses for it, by its Unicode range and the run's hint (Greek and Cyrillic letters are full width and dashes, arrows and symbols half width in MS Mincho and MS Gothic) (#167); a character a typeface lacks is drawn in the typeface Microsoft Word falls back to, character by character.
- .docx: changing the font writes the Latin typeface for both `ascii` and `hAnsi`, so a font change no longer keeps the old typeface for some characters.
- .docx: the automatic space between Japanese and Latin text goes beside é, α, а and half-width katakana and no longer beside ー (#166).
- .docx: PDF export no longer runs forever on a justified paragraph whose line ends with a soft hyphen (#161).
- .docx: line-breaking rules look past spaces (#162).
- .docx: the underline of vertical text is on the right of the characters (#160).
- .docx: kana with a combining semi-voiced mark (か゚ and others) are copied from the exported PDF as their characters (#163).
- .docx: the color of an equation's text is kept when the file is saved and opened again (#171).
- .docx: 縦中横 draws every digit of a run, not only the first (#124).
- .docx: text being composed by the input method is drawn on the line's baseline and laid out as typed, the page scrolls to it, and the candidate window opens under its last line (#121, #122); the composition's underline is on the right of vertical text (#138).
- .docx: the Compatibility Mode bar is not shown in reading view (#139); a line-number setting with countBy 0 no longer numbers every line (#140).
- .docx: bullets in Wingdings and Symbol are drawn and written as Microsoft Word writes them, and ■ ◆ ➢ ✓ of the bullet menu are no longer a box on Windows or other characters in Microsoft Word (#142).
- .docx: a paragraph with a deleted paragraph mark no longer shows as an empty line that cannot be deleted (#123).
- .docx: Enter on an empty list item of a lower level takes it one level up; only an empty item of the first level stops being a list item (#117).
- .xlsx: `=TYPE(+A1)` gives the type of the cell's value, not 64 (#169).
- .xlsx: `!` in a number format (TEXT and cell formats) shows the next character as is, as a backslash does, as in Microsoft Excel (#181).
- .xlsx: the error button (!) of a cell in the first visible column (column A, or the first column beside frozen columns) is drawn on the right of the cell, as Microsoft Excel does, instead of under the headers where it could not be reached (#183).
- .xlsx: the cells a formula spills into are formula results: a spilled text such as `=TEXT(SEQUENCE(5),"000")` no longer shows the number-stored-as-text mark, and Convert to Number is not offered there (#184).
- .xlsx: `LAMBDA` with an optional parameter (`[b]`) opens from Microsoft Excel's files and is saved as Microsoft Excel writes it, so Microsoft Excel no longer repairs the file (#168).
- .xlsx: the query editor's Applied Steps pane scrolls with the wheel and has a scroll bar (#172).
- .xlsx: Rename Sheet... from the menu gives the keys to the name field (#174).
- .xlsx: saving after changing a cell that a chart uses no longer rewrites the look of every series (since 0.8.5 a series' line could disappear in Microsoft Excel); only the chart's cached values change.
- .xlsx: a sheet with both a form control or picture and notes is saved in the order Microsoft Excel needs (Microsoft Excel replaced the sheet as unreadable); a book saved by an earlier version is mended when saved again.
- .xlsx: a million-row spill no longer slows the cursor keys (#137); editing an unrelated cell recalculates only the volatile formulas (#109).
- .xlsx: objects anchored to cells follow the rows measured on opening, and saving no longer anchors them to other cells.
- .xlsx: printing and PDF export place the rows as Microsoft Excel for Windows prints them (each row at whole printer pixels, also on a scaled page), so .xls and .xlsx pages break and line up closer to Excel's.
- .xlsx: pressing F2 or Enter in a cell no longer shrinks a row where the Normal font cannot be measured.
- .xlsx: the ink pen ends when the ink pane cannot be seen, and Erase All removes reopened ink.
- .xlsx: .xls files open and print closer to Microsoft Excel: shared strings no longer panic or lose text, files with Microsoft Excel's default password for a locked structure open, print settings, drawings (autoshapes, text boxes, lines, groups), pictures' crops, rich text fonts, conditional format colors and saved row heights are read; charts in .xls files show data labels, fills and lines, markers, axis number formats, secondary axes, display units and the axis titles.
- .pptx: .ppt files open and draw closer to Microsoft PowerPoint: placeholders, fills, lines, paragraph spacing, rulers, system colors, number styles, bullets, callout tips, gradients and the backgrounds follow the file.
- .pptx: text in columns that does not fit is shared over the columns as Microsoft PowerPoint does (#77).
- .pptx: the outline of text is stroked on the centre line of the glyph outlines over the fill, also in turned shapes (#129); the glow of text keeps its color (red glows were brown) (#91).
- .pptx: vertical text in a turned shape and line breaking by the run's language follow Microsoft PowerPoint (#155).
- .pptx: soft edge is drawn as Microsoft PowerPoint draws it for every kind of shape, line, fill, picture and group (#153).
- .pptx: a file with no theme colors uses Office's default theme, so a chart's automatic series colors are not black.
- .pptx: a shape that animates by letters or words keeps its fill, outline and text color (#89).
- .pptx: the last line of a justified paragraph is spread only before a line break; trailing spaces at the end of a paragraph are not underlined; a hanging indent makes the margin a tab stop on the first line; a missing Japanese typeface is drawn in Yu Gothic.
- .pptx: the format pane's Format Shape... opens the pane at the right, not the gradient dialog.
- macOS: the input method's candidate window opens under the last line of wrapped composing text, and unconverted text has a broken underline (#138).
- Windows: updating from the zip no longer deletes the files you keep beside the app (#136); opening a file in several threads at once no longer reads other column counts.
- Windows: a document protected with a password can be saved again, and a password to modify can be set (both failed on Windows).
- Web: a .docx font the Web build does not have falls to the browser's fonts, bold and italic reach the browser's drawing, and browser-drawn text is no longer heavier (#126).
- Web: playing a video or audio no longer freezes the page (#125).
- Web: the input method's candidate window opens under the composing text from the first key, and the clause underlines follow the browser's selection (#127).
- Web: glow, gradient and pattern text fills and text outlines are drawn (#129); vertical text takes the browser's vertical glyphs (#130); the slide show plays animations smoother (#128).

## [0.8.6] - 2026-11-06

### Added

#### All apps

- Replace Fonts is one shared pane in .docx, .xlsx and .pptx (Edit menu).
- Ink panel: the same rows in .docx, .xlsx and .pptx (select and clear ink, a line-width drop-down that shows the line).
- Open a file that is write-protected (.xlsx `fileSharing`, .docx `writeProtection`, .pptx `modifyVerifier`): it opens read-only until the password is entered, and it can no longer be edited and saved without it (#111).
- Check for updates: Help > Check for Updates in the three screens and the Start window. The Start window checks in the background whatever the start-up setting says, and shows an "Update ibisOffice" button when a newer version exists (#92).
- Modal dialogs close with Esc. A click outside an open menu or drop-down only closes it and does not reach what is under it.
- Number fields keep the exact value of the model: retyping or leaving a field unedited no longer rounds the stored value (glow, shadow and internal margins use it). When several objects are selected, a panel shows a value only when every object has it and writes only the fields you change; a mixed value is shown as empty.
- Shared slider with a hover highlight and a typed value (Enter confirms, Esc cancels); disabled sliders, spinners and segmented controls are drawn in the disabled color.
- A file changed by another program is detected: the window offers to reload it, or to save your changes under another name (a window that already shows the file is brought to the front).
- A save dialog that cannot open (for example on a network share) says so in a message instead of doing nothing (#110).
- Dark mode: a lighter selection blue in text fields and lists.
- .doc and .dot files open in all three screens (see Documents), and macOS lists ibisOffice under "Open With" for .doc and .dot files (#71).
- The About ibisOffice window has a Close button, and Esc or Enter closes it.

#### Documents (.docx)

- Word 97-2003 files (.doc, .dot) are opened with their formatting: fonts, styles, character and paragraph properties, sections and page numbers, tables (borders, floating tables, nested tables, merged cells, text direction), lists, headers and footers, footnotes, frames, fields, pictures, drawings (preset shapes, groups, rotation, text boxes) and the document's typography and compatibility settings (#71).
- Text columns: text flows line by line through the columns, the caret and Up/Down follow the columns, and the setting is saved (#77).
- A table of contents marked dirty (or with `updateFields`) is rebuilt when the file is opened (#112).
- Crop a picture in the document screen with the same in-place preview as the slide and sheet screens.
- The chart data of a chart in a document is edited in the spreadsheet grid, also for charts without an embedded workbook (#53).
- Fit Text (distribute characters evenly) has entries in the menu and the toolbar (#53).
- Tate-chu-yoko (horizontal text in vertical text) fits its width by a formula and the dialog sets its length and row thickness (#94).
- Quick parts offer the document properties.
- A vertical input method lays out its candidates vertically beside the column on Windows (#60).
- Shapes in .docx files read the full DrawingML shape properties (fill, outline, shadow, glow, soft edge, reflection and bevel), as slides do.

#### Spreadsheets (.xlsx)

- Notes are an editable yellow box: it appears when the pointer rests on the cell for one second, can be moved and resized with handles, holds rich text (font, size, color, bold, italic, underline) and is linked to the cell corner by a line.
- Copy as Picture puts the cells on the clipboard as a vector PDF, SVG and PNG, with cells that have no fill left transparent; a pasted PDF or SVG becomes a vector picture, and an EMF is written on Windows (#83).
- Pivot tables: expanding and collapsing items, field names and protection.
- Names Manager (with the scope of a name) and Paste Names (#100, #102).
- Cell Format pane with live update, a gradient slider and icons; the conditional formatting manager.
- Page Break Preview greys everything beyond the pages, and a new sheet's page range is its used range, which shapes put on the sheet enlarge (#104).
- LAMBDA helpers: a function can be passed as a value and is evaluated per element (`MAP`, `SCAN`, `REDUCE`, `BYROW`, `BYCOL`, `TAKE`, `DROP`, `XLOOKUP`, `HSTACK(SUM,MAX)`), checked against Microsoft Excel on hundreds of formulas (#98, #99, #106).
- `TAKE`, `DROP`, `INDEX`, `CHOOSE` and `CELL` return references, and `GROUPBY` and `PIVOTBY` match Microsoft Excel (#98, #99, #101).
- `TYPE` follows Microsoft Excel for ranges and unions (#69).
- Day and month names and era formats with a locale (`[$-411]`, `[$-ja-JP]`) are shown as Microsoft Excel shows them (#113).
- A sheet tab can be dragged only as far as the "+" button; the border button defaults to all borders; a form control's list hangs from its button (#95).
- Find shortcuts, and F2 edits the cell.
- Data validation with a half-width entry.

#### Presentations (.pptx)

- Animation pane: a table of the effects in a pane at the right (also opened from a sidebar button). Change an effect from a drop-down, delete it in the edit dialog, play all with the total time, and set the start trigger with radio buttons (#38).
  - Rows are reordered by drag and drop: the row is lifted under the pointer with a shadow, the other rows make room while it moves, and dropping never loses or merges effects.
  - A text box row folds and opens its paragraph rows, and a paragraph row moves within its text box.
  - Video and audio have their own rows; the space key plays or pauses the selected clip, and the Animation menu has Play, Pause and Stop for it (#86).
- Slide transitions: 55 transitions with Effect Options and the direction, and animations keep their `behaviors` when saved (#88, #89, #90).
- Slide Zoom, Section Zoom and Summary Zoom: read and kept as Microsoft PowerPoint writes them, made from the Insert menu with a picture of the target, zoomed in and out in the slide show with the transition time, and the summary section and its slide are kept up to date.
- Slide Master view: the left pane lists every master with its layouts, thumbnails switch at once, and Close Master View (and the close buttons of the notes and handout masters) returns to the slides; Apply and Close applies the layout you were viewing (#84).
- Guides: the guides of a master, a layout and a slide in a .pptx file are read, saved and drawn, and can be added, moved, deleted and snapped to (#84).
- Text glow is drawn as Microsoft PowerPoint draws it: a Gaussian blur whose size follows the font size (measured on 176 samples), also in the Web version (#91).
- Shadows: a blurred shadow is a Gaussian of the outline, follows the fill's transparency and is also cast by shapes without a fill; an inner shadow is exported to PDF (`a:innerShdw`).
- Right-pane panels for glow, shadow, internal margins and the 3-D model use number fields with decimals, color fields and sliders; the pattern fill drop-down shows the 54 patterns; the Show Settings and Print panels were rearranged; recording preview is Click or Auto with a 0.5 s stepper that applies live.
- Pie charts: the percentage labels add up to 100% as in Microsoft PowerPoint, and the label separator follows it.
- Cropping keeps what stays where it was, and a picture turns with its handle.

#### Charts (all formats)

- Chart data without a workbook is edited in a grid, in .docx and .pptx files too (#53).
- The chart's default look follows Microsoft PowerPoint: no fill without `spPr`, the 2-D `c:style` defaults (line color, bold, trendlines), the automatic value-axis ticks, and 3-D boxes drawn by projection with `hPercent`, clustered 3-D columns, and no fill on empty walls.

#### Web version

- Half-width Latin letters and digits are drawn with the font installed in the browser instead of IBM Plex Sans (#73).
- Images are written to the clipboard (#83).
- The About window is a modal dialog.
- With `view=1` (and `--read-only`) the read-only information bar of the document screen is not shown, as in the other two screens; the file's own bars (final, editing restrictions) still show (#72).

### Changed

- Duplicate menu commands were removed in the three screens.
- Line breaking of .docx, .xlsx and .pptx files follows one set of rules for the characters that cannot start or end a line, hanging punctuation, squeezing and the automatic spaces between Japanese and Latin text (#68).
- The Animation pane's move arrows, trash button and play-order column were removed; reorder by dragging.
- .docx tables that are not fixed-width are laid out from their cells' widths and content, as Microsoft Word does.
- A justified line's spaces: the automatic spaces between Japanese and Latin text widen first, up to half an em, then the rest of the room is spread over every gap (#67).

### Fixed

- All apps: a click in an open menu or a drop-down no longer passes to the cell or shape under it; the Start window's "..." menu no longer closes by itself when the mouse moves; the window's close button asks to save before closing a changed document (#38).
- .docx: justified lines break where Microsoft Word breaks them, with the last word taking the room that the spaces give (#67); lines of a multiple of the line grid, line heights of East Asian faces (1.3 times for Meiryo, Yu Gothic, Yu Mincho and others), character-grid spacing, empty paragraphs, footers that take room off the body, and the first-page header's room were matched against Microsoft Word's own PDFs, so documents break onto the same pages (corpus of 150 documents: the pages and lines agree more often); a character that the vertical text squeezes is turned like the others (#54); an emoji is drawn in color and not squeezed into the run's cell, and a character with a variation selector stays one cluster, also on Windows (#66, #79); the bullets of Wingdings and Symbol are drawn with their own glyphs; an inline picture fills its frame; an installed proportional MS Mincho or MS PGothic keeps its own widths (#56); a shape's text, group and picture inside a group are placed as Microsoft Word places them; footnotes and endnotes keep the position and widow control Microsoft Word gives them; text broken between Latin letters when "break Latin words" is on; the underline of spaces at the end of a line follows `ulTrailSpace`; merged cells with centered, bottom-aligned or turned text stay with their first row; a font table is kept whole on save.
- .xlsx: Excel 2007 form controls (VML only) are drawn (#95); a tab character in pasted text no longer crashes (#108); a file that stayed on "loading" because of a column range reaching 4.3 billion opens (#82); `IF` evaluates a branch once so LAMBDA recursion no longer slows down, and `XNPV` and `XIRR` evaluate their arguments once (#109); the printed layout follows Microsoft Excel: rows by their fonts, custom rows cut to whole points, the printed column width, a Letter-sized layout on A4 for sheets without a paper size, the default paper of the region, text breaking in the printer's units, shapes following their rows and charts, rows below the last cell, and a blank last page (#93, #68); a distributed (justify) cell spreads its characters (#75); a wrapped cell no longer ends a line at its last space; a copied sheet's "(number)" counts up without a limit (#78); sorting moves a row's hyperlinks with the row; a pasted picture keeps its size as in Microsoft Excel (#103); a hidden group box no longer steals clicks from the option buttons in the Web version (#96); a long formula that is confirmed and edited again no longer shows `####` in the cell, the cell's editor widens to the window's right edge and wraps the formula (#105); `NOW` reads the time at every call (10 ms steps), also inside a LAMBDA, instead of changing once a second (#115).
- .pptx: the presenter view with two displays no longer aborts (#51); a text in a custom shape whose text rectangle has no area is one line, not one character per line; fonts of a variable font are written at the weight they are drawn at; text shrunk by autofit is drawn at a whole-point size and "space before" is rounded to a whole point as Microsoft PowerPoint does; table borders keep their thickness in PDF; pattern fills are one picture in PDF; the media bar's time fits the bar; placeholder buttons have the toolbar size; the slide master's thumbnails switch without rebuilding the pictures; dashed lines are written to PDF as dashes with their pattern; a new layout adds the placeholders the slide lacks, empty (#85).
- Windows: the save dialog opens for a file on a network share (#110); the title row's icon shows the system menu without the app being killed (#74); emoji that Unicode draws as emoji by default and color emoji (COLR version 1) are drawn, and kana in an emoji run use their fallback font's width (#66); printing opens the PDF app also for a file whose name contains "test" (#19); the colors of an embedded video are no longer inverted (#87).
- macOS: in Dark Mode the font names in the font drop-down are drawn in the theme's text color, and a color picked in the color swatches is no longer inverted (#114).
- Linux: the pen and touch of a Wayland session (Wacom tablets) work (#97).
- Web version: half-width Latin uses the installed font (#73); copied images reach the clipboard (#83); the About window shows as a dialog.

## [0.8.5] - 2026-10-30

### Added

#### All apps

- Recent files: a "..." button appears at the right of a row on hover (or right-click the row) with "Remove from list". Only the history entry is removed, never the file (#61).
- A new file is added to the recent files list as soon as it is saved for the first time (#61).
- The menu bar shows the ibisOffice logo at its left; clicking it opens the Start window. It is hidden when the window is too narrow.
- Number fields in the right-pane panels have up/down arrows that step the value and keep its unit (cm, pt, %, and so on), for example paragraph spacing and indents, picture and shape size, rotation and position, table row height and column width, page setup, durations and the chart's gap, overlap and 3-D rotation. Text fields share one look, one width and a clear (x) button.
- A color picker dialog (hue wheel) opens from "More Colors..." at the end of every color palette and from the color code fields.
- The caret blinks the same way in cells, text, shapes and the formula bar.
- A shape or text box with text is edited with a single click, also right after it is inserted; dragging several selected shapes keeps the selection, and the rotation and square handles follow the shape's angle.

#### Documents (.docx)

- Fit Text (distribute characters evenly): a dialog sets the width per character with OK, Cancel and remove; the target shows a light-blue underline at the caret; the setting is saved, reloaded and undone (#53).

#### Spreadsheets (.xlsx)

- Data validation lists: the choices narrow as you type, Enter confirms, and a rejected entry is cleared with the error shown in a dialog (#33).
- Ctrl+: enters the current time (#34).
- F4 repeats the last action (formatting, inserting rows and so on) on the current selection, also when several formats were applied together (#35).
- Formula entry: function name completion, picking references with the arrow keys (also with Shift and Cmd/Ctrl+Shift), seven reference colors, an animated dashed outline, dragging the corners of a reference, automatic closing parentheses, and Enter, Tab and Esc (#52).
- Long formulas wrap in the cell and in the formula bar. In the formula bar, Up/Down move by display line (or by one page of width in a single line), Home/End go to the start/end of the line, Ctrl/Cmd+Home/End go to the start/end of the whole formula, and Ctrl/Cmd+Shift+Left/Right select by word (#65).
- A typed value that starts with a sign (for example `+1+2`) is read as a formula, while negative numbers and percentages stay numbers and a leading apostrophe stays text (#52).
- Entering a number widens a column that was not sized by hand so the number fits (a column is never narrowed, and an automatic width is not saved, as in Microsoft Excel) (#64).
- Pasting shapes copied from Microsoft Office gives editable shapes with their text; copying offers the native shapes, a picture and the text (#41).
- A cut source keeps its dashed outline while the selection moves.
- Sort is a pane at the right instead of a dialog: levels can be copied, edited and reordered, a level can sort by cell color, font color or icon, "Use the first row as headers" replaces the on/off labels, and "Update in real time" (on by default) applies changes as you edit.
- Header and Footer editing in Page Layout view uses a pane at the right, with the left, center and right boxes on the paper, instead of a dialog over the paper.
- The Watch Window no longer blocks the sheet; you can keep working in the workbook while it is open.
- The Scenario Manager has a Close button.
- Charts: a background color field under the legend, "use the first cell as the series name", the source range is kept when it is edited, and whether the first row or column is a header is decided from the data types (#43).
- A sheet tab that is partly hidden scrolls into view with room for its neighbours.

#### Presentations (.pptx)

- Video and audio: the play timing is saved (`p:cMediaNode`, `mediacall`, `togglePause`) so Microsoft PowerPoint plays the clip, volume, loop and the other settings are written where Microsoft PowerPoint reads them, and the animation is kept after editing it (#62).
- A control bar under a selected video or audio shape in editing view: seek bar, back to start, play/pause, mute, volume and the time (#62).
- The compatibility check reports video and audio that are linked and not embedded (#62).
- Audio and video play on Windows, with the play position and the end of the clip; video frames are drawn in the slide (#62).
- Ctrl+M adds a slide; Ctrl+D duplicates the selected shape, or the slide when no shape is selected (#62).
- Pictures cropped to a custom shape are clipped to that shape (#62).

#### Web version

- The release now attaches the Web build for embedding: `ibisOffice-<version>-Web.zip` (the same files as the playground's `app/`), with `ibisOffice-<version>-Web.zip.sha256` (the archive's SHA-256) and `ibisOffice-<version>-Web.files.sha256` (the SHA-256 of every file in it), so you can pin a version and check what you fetched. See docs/web-embedding.md (#76).

### Changed

- The Start window's logo moved from the title row to above "Please support us".
- Standard-format numbers: the minus sign is no longer counted in the 11 characters shown before switching to scientific notation (#64).
- The Header and Footer, Sort and Watch panels no longer block the document (see Added). Dialogs that ask a question stay modal.

### Fixed

- All apps: dragging a scroll bar keeps hold of it when the mouse leaves the bar (#39); drop-down lists are sized by their content.
- .docx: vertical text uses the font's vertical forms (`vert`/`vrt2`) for punctuation, brackets, quotation marks, units and symbols, and turns characters that have none by 90 degrees as Unicode (UAX #50) specifies; text being composed with an input method is drawn vertically in its color; line spacing set as a multiple of a large font size follows the multiple (#54, #60).
- .xlsx: Filter on a sheet of about 40,000 rows no longer freezes (#37, #49); changing a spill of a million cells no longer hangs, and `SEQUENCE` checks its size limits before allocating (#44); `TAKE` and `IF` keep the reference they return, and `TAKE` evaluates its input once (#45, #50); `LAMBDA` accepts an optional last parameter such as `[b]` (#46); Trace Precedents arrows no longer remain on other sheets (#42); headers and cells stay aligned far from the origin when zoomed (#47); furigana rows keep room for the text and can be edited again (#40); editing a cell range keeps the data validation rules outside it; renaming a sheet by double-click needs both clicks on the same tab, also after the tabs scrolled.
- .pptx: text in rotated shapes with two or more lines rotates about the center of the whole shape, so lines no longer collapse or overlap at 90, 180 and 270 degrees (#48); the Zoom and Float entrance animations use the right preset numbers, and Microsoft PowerPoint's own Zoom (preset 53) is read as Zoom, not Fade (#62); a new layout drops empty placeholders it has no counterpart for and keeps filled ones (#62); the slide background is inherited from the layout and master, white when none is set, and `useBgFill` shapes take the slide background (#62); the glow of text starts at the outside edge of the glyph outline (#62); "Start from Presenter View" no longer crashes with several displays (#51).
- Cropped pictures keep the resolution they need instead of looking blurred (#55).
- Windows: bold and italic that DirectWrite simulates are kept in PDF export and printing (#57).
- Shapes and charts keep their native drawing and chart editing data across saves.

## [0.8.4] - 2026-10-23

### Added

#### All apps

- Closing a window with unsaved changes asks whether to save them — with the window's close button, ⌘W / Ctrl+W (File > Close) and Quit, in .docx, .xlsx and .pptx files and in new files. Save writes the file first (a new file asks where), and Cancel keeps the window (#27).
- Open in Microsoft Office: a title-row button opens the saved file in Microsoft Word, Excel or PowerPoint, asking where the app is when it is not found (#26).
- Dark mode: when the system is dark, the window frame (title row, menus, toolbar, panels, settings, dialogs) is drawn dark and follows changes at once; the document stays white, as in Microsoft Office. The web version follows the browser's setting.
- Settings: "Always show scroll bars".
- Save As offers the file types in the dialog (macOS and Linux): .docx, .dotx, .odt; .xlsx, .xltx, .xls, .ods, .csv, .tsv; .pptx, .ppsx, .potx, .odp. A typed extension is kept.
- Save and open OpenDocument text (.odt), and save OpenDocument presentations (.odp).
- Menu search: a toolbar button and Help > Search Menus find any menu command; the search list can be used with the keyboard (↑↓, PageUp/PageDown, Enter).
- A Window menu: the open windows, Arrange All and Cascade.
- The font size in the toolbar is a box you can type into, with a drop-down list of sizes.
- Text fields have a clear (✕) button; long menus and submenus scroll, with arrows at the top and bottom and a scroll bar.
- Shapes and text boxes in .docx, .xlsx and .pptx files: the same in-place text editing everywhere (caret, selection, input methods, Undo per typing session), eight resize handles and a rotation handle, Esc cancels a drag, Shift+click selects several shapes, and formatting (bold, italic, underline styles, alignment, indents, Clear Formatting) applies to all the selected shapes. Corner handles of pictures keep the aspect ratio.
- Color choices in menus are shown as color palettes instead of color names.
- macOS: double-clicking the title row zooms or minimizes the window as set in System Settings.
- Printing on Windows and Linux: the document is written to a PDF and opened in the system's PDF app (also Mail Merge) (#19).
- The About window shows the ibisOffice logo.

#### Documents (.docx)

- Double-click a text box to edit its text in place; the look before and after editing matches. The mouse pointer changes over the handles and the frame.
- The text box panel applies changes as you type (no Apply button), follows the selected or dragged box, and has a line style drop-down with pictures. Newly inserted text boxes are centered in the column.
- The toolbar has Fill Color and Line Color for the selected shape; bullets, numbering, checklists and Clear Formatting work in text boxes.
- Microsoft Word for Mac shortcuts: ⌘⇧D (double underline), ⌘⇧W (words only), ⌘⌥F (footnote), ⌘⌥E (endnote), ⌘⇧L (bullets).
- Menu commands that did nothing are connected: Page Setup, Dictionary, Accessibility and Full Screen. View modes are grouped in a View Mode submenu.
- Fields KEYWORDS, LASTSAVEDBY, NUMCHARS, NUMWORDS and REVNUM show their values.
- `w:fitText` (fit text to a width) is read, drawn and saved.
- The default zoom is 141%.

#### Spreadsheets (.xlsx)

- The Select All button at the top-left corner of the headers selects every cell (#31).
- Spill ranges: an outline and shadow around the spill range, a dashed outline for #SPILL!, and the formula shown greyed in the formula bar for the other cells of the range (#24).
- Drag the border of a selection to move cells, or hold Option to copy them, with Microsoft Excel's pointers and its question before overwriting cells (#11).
- Paste Options after pasting (formulas, values, formats, no borders, transpose, paste link and more; one Undo), and a Paste Special dialog.
- Insert Function and Function Arguments (fx, ⇧F3): search functions by name or description, see the result as you type the arguments.
- Tools > Word Count and Thesaurus; Recommended Charts, and every chart type in Insert > Chart.
- The right pane shows the attributes of what is selected (cells, a chart or a shape) instead of tabs; shape values apply as you type. New text boxes have a light yellow fill and a thin black line.
- The status bar's Average, Count and Sum stand at the right, next to the zoom.
- Menus are tidied into submenus (Insert, Delete, Sheet, Comments, View Mode); the sheet list is taller and scrolls; sheet tab colors are drawn under the name.
- Page Break Preview: manual page breaks as a blue line and "Page N" watermarks; the pages follow manual breaks (#22).
- Split view: scroll bars in each pane, and the pane follows the active cell.
- Cell pattern fills use Microsoft Excel's 17 tiles; printed underlines match Microsoft Excel's thickness and position.

#### Presentations (.pptx)

- Right-click menus for the Link, Header and Footer, Notes and Handouts, and Properties panels.
- Pie charts: exploded slices (`c:explosion`).
- The 74 built-in table styles are drawn as Microsoft PowerPoint draws them (tints and shades, banding, first/last column, total row, lines).
- Text kerning (`kern`) is applied.
- Picture outlines are centered on the picture's edge and can be dashed.

#### Charts (all formats)

- Trendlines in the legend; legends wrap to one column when they do not fit.
- Two-level category axes, dashed series lines, gridlines and axes (`a:prstDash`), the value axis title's direction, plot area borders, data label boxes, `tickLblPos=low`, error bars and data tables.
- Charts without cached data read the values from the cells.

#### Web version

- Close and Download buttons, turned on by start options, which tell the embedding page by events and `postMessage`.
- `view=1` opens the document read-only, with the menu bar and toolbar shown.

#### Command line

- `--read-only` is documented in the command-line options.

### Changed

- ⌘W / Ctrl+W closes the window in every format instead of showing the start window; the app ends with its last window. The start window closes with ⌘W too.
- The right pane's title-row button shows whether the pane is on screen and is disabled in a .docx file while no panel is open. The left pane's button is called "Navigation".
- Windows and Linux: shortcuts are shown as Ctrl, Shift and Alt instead of macOS symbols (#28).
- Checkboxes and radio buttons have one look across the app; panels keep their action buttons visible at the bottom.
- .xlsx: Quick Analysis no longer appears on selecting cells; .docx: the floating mini toolbar no longer appears on selecting text.
- Column widths and row heights follow the desktop Microsoft Excel for Windows (digit widths measured as Microsoft Excel does).

### Fixed

- .docx: body text no longer overlaps a header taller than the top margin (#10). Text box placement and the panel following the box (#14).
- .docx layout, compared with Microsoft Word on real documents: line heights of Yu Gothic and MS fonts, the document grid, character spacing, Latin character widths as Microsoft Word measures them, punctuation compression and hanging punctuation, justification, floating tables (placement, wrapping, splitting across pages, in table cells), tables wider than the page, columns, vertical text in merged cells, headers and footers with empty paragraphs, superscripts and footnote line spacing, kerning with Japanese text, text box contents (underlines, line breaks, hyperlinks, tables), VML pictures and shapes, Wingdings symbols, and font names saved in the wrong encoding.
- .xlsx: a date or formula typed into a cell formatted as Text stays text, also when pasted (#13). A crash when switching sheets with Japanese text after `#` in a formula (#21). CSV dates and multi-line cells, row heights while editing, and column AutoFit (#17). Conditional formats that use defined names (#23). The sheet list scrolls in workbooks with many sheets (#20). Page Break Preview and printing closer to Microsoft Excel: page sizes, pictures without a transform, charts and shapes on the pages (#22). Wrapped numbers fill with `#`, 3D chart walls, all-zero value axes, scatter charts, embedded OLE pictures, headers and footers with line breaks and `_xHHHH_`, page numbers across sheets.
- .pptx: a shape whose shadow was removed (an empty effect list) no longer gets the theme's shadow (#16). Background fills from the theme, tables without a style, picture fills with crop, tile, transparency and recoloring, CMYK JPEGs, EMF pictures, text shadows and glow, text outlines, hidden slides in export, bullets in points, numbered bullets, rotated text near the slide edge, pictures with transparency casting shaped shadows, and fonts with weight names such as "Segoe UI Semilight" being reported as missing (#30).
- macOS: bold and italic for Japanese fonts without those faces, also in PDFs (#18). A crash on quit when releasing the Touch Bar (#17; thanks to uchidama). A crash when the open or save panel could not be created.
- Windows: paths no longer start with `\\?\`; Japanese sort order; printing on Windows 11 in Parallels (#19).
- Memory: opening a 1,048,576-row CSV uses 4.1 GB instead of 7.0 GB; PDF export of large workbooks is faster and of presentations uses less memory.

## [0.8.3] - 2026-10-18

### Added

#### All apps

- A desktop app for Linux (64-bit x86_64; Ubuntu 20.04, Debian 11 or later): `ibisOffice-<version>-Linux.AppImage`. It needs no installation and no FUSE: at the first start of each version it unpacks itself into `~/.cache/ibisOffice/app/`, and starts from there afterwards.
  - At its first start, ibisOffice offers to add itself to your list of apps; it then also appears under "Open With Other Application" for .docx, .xlsx and .pptx files.
  - Automatic update on Linux: the new AppImage is checked against its SHA-256 and takes the place of the one you run. The log is `~/.local/state/ibisOffice/updater.log`.
  - Exporting to PNG and PDF from the command line (`--export`) works on Linux too.
  - Where Microsoft Office's fonts are not installed, fonts with the same widths are used: Liberation (Times New Roman, Arial, Courier New), Caladea (Cambria), Carlito (Calibri, bundled) and Noto CJK (Japanese). Kanji are drawn with Japanese glyphs, and text being typed with an input method uses the same font and no longer overlaps.
- Pinch to zoom with two fingers on a macOS trackpad, around the pointer, in .docx, .xlsx and .pptx files (#2).
- Linux and Windows: the title bar has Minimize, Maximize and Close buttons, and you can drag the window by it (#7). On Windows, the ibisOffice icon at the left of the title row opens the system menu; with the menus hidden, the toolbar becomes the title row on Windows and Linux.
- The font list is built from the fonts installed on your computer and the bundled fonts, sorted in Japanese syllabary order, with a "Recent" section, and is faster to open. A font chosen while editing a cell shows at once (#5).
- Spell checking with one shared panel in .docx, .xlsx and .pptx files (body, tables, headers and footers, notes, comments, shapes; Change, Change All, Ignore, Add to Dictionary). The proofing language and the AutoCorrect options are shared too.
- AutoCorrect while typing in all three formats: symbols such as (c), (r), (tm) and -->, capitalizing sentences and days, fixing two initial capitals and Caps Lock, and your own replacements.
- Auto-recovery saves your work regularly in .docx, .xlsx and .pptx files, with a recovery panel and bar.
- Split toolbar buttons where Microsoft Office has them (bullets, numbering, underline with 17 kinds, accounting format with currencies, shading, borders, Merge & Center, AutoSum, New Slide); the left half repeats your last choice.
- Mac keyboard shortcuts while editing text (⌘←, ⌘→, ⌃A, ⌃E and others) and View shortcuts for zoom.
- In the English user interface, font names are shown in English, and new pictures and charts are named "Picture n" and "Chart n".
- A command-line option, `--layout`, now also writes where the text is drawn and whether it overflows its frame (#8).
- Finder's "Open With" offers ibisOffice for .csv, .tsv, .odp, .otp, .ppt, .pps, .pot, .xls, .xlt, .ods and .ots files, and Windows registers ibisOffice for .csv and .tsv; ibisOffice does not take over the default app. The Open dialog shows only the files ibisOffice can open.
- EMF, WMF and EMF+ pictures are drawn (lines, fills, paths, text, bitmaps, clipping), with text in the font's character set (Shift_JIS, Big5, GBK, EUC-KR and Windows code pages).
- The start screen has a "Get involved" panel with a link to GitHub Issues.
- Windows: a new app icon, and version information in the program's Properties.

#### Documents (.docx)

- Mail Merge: address block and greeting line, Match Fields, rules (If…Then…Else, Compare, Skip Record If, Next Record), adding, removing and filtering recipients (several conditions), preview and find, merging to separate files, a range of records, printing and PDF, envelopes and labels, and Check for Errors. The document type is saved.
- Equations: .docx equations (OMML) are laid out and drawn, with built-in equations, accents, limits and binomials, linear, skewed and no-bar fractions, equation AutoCorrect, line breaks and alignment, and editing in place. `Eq` fields are drawn as equations.
- Shapes: draw shapes by dragging (Insert > Shapes), type text in them and format part of it, elbow and curved connectors that stay attached when you move a shape, group and ungroup, vertical text alignment, crop to shape, edit wrap points, and line, gradient, shadow, glow, soft edge and reflection settings. VML shapes from older documents are drawn (rectangles, ovals, lines, paths, fills, shadows, text boxes), and text boxes can be vertical.
- Pictures: Reset Picture, picture styles, corrections, color, transparency, artistic effects, compression settings, SmartArt and signature lines.
- Review: Compare Documents (a new document, tables cell by cell, the 8 compare options), Combine Documents, View Side by Side with synchronous scrolling, a grammar and style checker with its Review panel, the Thesaurus, open passwords, tracked moves (`w:moveFrom`, `w:moveTo`), Next/Previous Footnote and Comment, and Print Markup.
- Comments: the commented text is highlighted, a marker in the margin, and moving between the comment and its text both ways (#1); comment balloons outside the page, replies, and delete confirmation.
- References: Index (XE, Index), Table of Authorities and Table of Contents entries (TC, TA, TOA), citations and bibliography with the Source Manager, AutoText and building blocks, caption numbers with chapter numbers, and outline collapse and expand (saved). Headings can be moved by dragging in the Navigation Pane and with keys.
- Content controls: title, tag, lock, placeholder text, picture controls, building block galleries, repeating sections, groups, legacy form fields and Design Mode.
- Document design: themes, theme colors, theme fonts, effects, style sets, paragraph spacing presets and Set as Default; list styles; drop caps.
- Document templates: Save as .dotx, File > New from Template, attach a template and update styles. Compatibility mode with conversion.
- Text effects: Microsoft Word 2010 outline, glow, shadow and gradient fills, emboss and engrave, shading patterns, wavy borders, ligatures and OpenType features, and numbering in Korean, Chinese and Hindi.
- Pages: multiple pages and book fold, a cover page, a blank page, the Dark Mode page, footnote separators, and printing page ranges, both sides, collated copies and several pages per sheet.
- Insert > Chart, editing chart data and changing the chart type, also for charts already in the document.
- Editing: AutoFormat as you type (smart quotes, automatic bullets and borders), editing in several areas at once, drag-and-drop editing, picture bullets, Draw tab (pens, highlighter, eraser, lasso, ink to shape), Immersive Reader, ScreenTips, and Microsoft Word's keyboard shortcuts.
- Files: "Always create backup copy", auto-recovery interval, embedding fonts, Inspect Document and the Compatibility Checker, Open and Repair, master and subdocuments, and saving as text, RTF, Microsoft Word 97-2003 (.doc), HTML, OpenDocument and PDF.

#### Spreadsheets (.xlsx)

- Open Microsoft Excel Binary Workbooks (.xlsb) and password-protected Microsoft Excel 97-2003 workbooks (.xls); save as Microsoft Excel 97-2003 workbooks (.xls, with comments and pictures) and OpenDocument (.ods). Open and save .ods charts, shapes, pictures, conditional formats, data validation and comments.
- Charts: Waterfall, Histogram, Pareto, Funnel, Box & Whisker, Treemap and Sunburst, also from Microsoft Excel files, with their format settings; 3D column, line, pie, area and bubble charts with 3D rotation; radar, surface, stock, combo and bar/pie-of-pie charts; chart sheets and charts that follow cell edits, row and column changes and sheet renames. Charts in older .xls files are read. Change Chart Type, Quick Layout and Switch Row/Column.
- Slicers and timelines (insert, settings, styles, report connections, date grouping), Form Controls (10 kinds, with a properties panel and Design Mode), SmartArt, pictures in cells, screenshots, and removing picture backgrounds.
- PivotTables: recommended PivotTables, 13 styles, report layouts, banded rows and columns, subtotals, blank rows, repeat item labels, change data source, drill buttons, GETPIVOTDATA and PivotCharts.
- The Solver (Simplex LP, GRG Nonlinear, Evolutionary, with answer, sensitivity and limits reports), the Data Analysis tools (Data > Data Analysis…, 19 tools, with chart output) and Forecast Sheet (FORECAST.ETS with seasonality).
- Query Editor: applied steps, types and locales, column and row operations, split and merge, replace, pivot and unpivot, group, merge and append queries, custom and conditional columns, parameters, M code, and blank queries. Text/CSV import with fixed width, a new sheet and a table preview.
- Page Layout view with rulers, headers and footers you type directly, and column and row headings on each page.
- IMAGE function (pictures from a URL), structured references shown as `[@Column]`, LINEST and LOGEST statistics, BESSELI/J/K/Y, real random numbers (RAND, RANDBETWEEN, RANDARRAY), CELL information, and function category submenus. All 23,224 function test cases match Microsoft Excel (financial, statistical, engineering, lookup, math, text, information and more).
- Links to other workbooks: a bar at startup (Update Values, Edit Links), changing the source, and formulas that point to another workbook add the link.
- Large sheets: opening a 121 MB CSV with 1,048,576 rows takes 4.6 seconds instead of 62.9; changing number formats and AutoFit columns are many times faster; moving, PageDown and editing work up to row 1,048,576, with a row header that widens with the number of digits. The grid grows as you scroll past the used area.
- Editing: Insert and Delete Cells in one dialog, Paste Special with options (⌘⌥V), copying and pasting several ranges, deselecting with ⌘-click, Pick from Drop-down List, Flash Fill, fill in four directions, Find and Replace options, row and column size dialogs, double-click a row border to AutoFit, more Freeze Panes options, window splitting, and Text to Columns with column formats.
- Draw tab: pens, highlighters, pen effects, eraser, lasso selection and ink to shape. Sparklines with markers, axes and colors; conditional formatting presets; tables with total row drop-downs; print area tools; comments and notes in the right-click menu.
- Options: AutoCorrect, error checking, Save, Trust Center and the Quick Access Toolbar; Export > Without Macros.
- Microsoft Excel 2010 data bars and icon sets are read and drawn. Number formats follow the locale in `[$-…]` (month and day names in German, French, Spanish, Italian, Portuguese, Korean and Chinese).

#### Presentations (.pptx)

- Open Microsoft PowerPoint 97-2003 presentations (.ppt, also password-protected) and OpenDocument presentations (.odp, .otp), and save as .odp. Animations, shadows, numbered bullets and cropped pictures in .ppt files are read, and .odp animations are read and written.
- Equations: Insert > Equation… with linear UnicodeMath and LaTeX, laid out in 2D as in Microsoft PowerPoint (fractions, roots, matrices, functions, limits, accents, equation arrays), with a live preview and editing inside an equation.
- Outline view: edit text, change levels, move and merge slides, and select several lines.
- Notes and Slide Master: the Notes pane has an on/off button and a close button, panes are resized by dragging, and the Notes Master and Handout Master are edited and saved. Slide thumbnails, the slide canvas, shapes, tables, pictures, charts and text have fuller right-click menus.
- Draw tab: pens, pencil, highlighters, pen effects, eraser, ink to shape, text and math, Ink Replay, and the Ruler.
- WordArt: 41 kinds (text warp presets), drawn as in Microsoft PowerPoint. 3D bevels, 3D rotation and depth, curve handles in Edit Points, connector glue, and the 140 preset color names.
- SmartArt: insert presets, convert bullets, add and remove items, the text pane, layouts, color styles, and animation by element.
- Animations: by word and by letter, chart animation by series and category, group effects, Spin and Grow/Shrink in the slide show, and Morph with words, letters and colors.
- Pictures: color effects, crop to shape, tiles, artistic effects (22 kinds), background removal, Photo Album and screenshots. Themes: save .thmx and theme effects. Compare and merge presentations, embedded fonts, Zoom and high contrast view, audio recording and media compression.
- Chart types Box & Whisker, Treemap, Sunburst, Waterfall, Pareto and Funnel, with data labels, axes and fills.

### Changed

- .docx and .pptx: 100% zoom now shows 1 pt as 1 px, the same as Microsoft Office for Mac.
- .xlsx: the formula bar works like Microsoft Excel's (a cell with line breaks shows one line; ▼ expands it to four lines). The zoom percentage in the status bar is one button, and the Fit button is removed. Zooming out with the wheel is faster.
- .xlsx: the cell editing box is filled with the cell color, shows no green selection border, and keeps the cell's alignment.
- .xlsx: chart axes follow where they cross (`crosses`, `crossesAt`), and date axes thin out and rotate their labels as Microsoft Excel does.
- .pptx: backgrounds and theme colors changed in Slide Master, Notes Master and Handout Master view are saved back to the file.
- Printing and PDF export: text larger than 32 pt is not drawn when it is more than 1.3 times the row height, as in Microsoft Excel.
- On macOS, the Start, Undo and Redo buttons no longer overlap the window buttons.
- The version is 0.8.3 on every platform (macOS, Windows, Linux and the web version).

### Fixed

- .xlsx: a cell with many line breaks (for example in a large CSV file) is no longer drawn over the toolbar and the formula bar while you edit it, and its row stops at the maximum height. Merged cells keep their size while editing (#6).
- .xlsx: pressing Enter while editing confirms the cell and moves to the next one; a long pasted text stops at the maximum row height (409.5 pt); lines in the editing box stack vertically.
- .xlsx: the font list is no longer drawn black. Dragging a row border stops at the maximum height. Moving the caret no longer shifts the surrounding text (.docx, .xlsx and .pptx).
- .xlsx: rows whose height is not set no longer grow for wrapped text in merged cells; CELL("filename") and CELL("address") work per workbook; AutoFilter, page breaks and protection in .xls and .xlsb files are read. Files saved with pivot grouping, sparklines, new error types, phonetic settings, SVG pictures and custom views open in Microsoft Excel without repair.
- .docx: borders, text boxes, glow, line spacing and numbering characters (Korean, Chinese, Hindi) are closer to Microsoft Word's layout. Saving a new header with a table no longer fails.
- .pptx: the cloud shape is drawn correctly (#4). Slides pasted from other presentations no longer carry custom XML, and 3D rotation angles are saved in a range Microsoft PowerPoint accepts. Picture tiles, gradient paths and text on a circle are closer to Microsoft PowerPoint. Files with SmartArt, transition sounds and custom shows open in Microsoft PowerPoint without delay or errors.
- Web: a build error in the playground is fixed; the sample date format is corrected.
- Linux: window buttons and dragging work with client-side title bars; build errors on Linux and Wayland are fixed (#3).

## [0.8.1] - 2026-10-12

### Added

- A desktop app for Windows (64-bit): `ibisOffice-0.8.1-Windows.zip`.
- Automatic update (macOS and Windows). At startup, ibisOffice checks for a newer version, downloads it and asks whether to update. A separate program, `ibisOfficeUpdater`, replaces the app and opens the new version again. If the new version does not start, it puts the old version back.
  - The new macOS app is installed only if it is signed by ibis inc. and notarized by Apple. On Windows, the download is checked against its SHA-256.
  - If an update fails, a button opens the download page.
  - Each step of an update is written to a log (`~/Library/Logs/ibisOffice/updater.log` on macOS, `%LOCALAPPDATA%\ibisOffice\Logs\updater.log` on Windows). Help > Show Update Log opens it.
  - Settings: "Check for updates at startup" (on by default).
  - After two failed attempts at the same version, ibisOffice stops offering that version and asks you to download it again.
- Color palettes, as in Microsoft Office (theme colors and standard colors), for the text, fill, highlight and border colors in .docx, .xlsx and .pptx files.
- Split toolbar buttons for colors: the left half applies the color you used last, and the ▼ opens the palette.
- .xlsx: a new Format Cells pane.
  - Number, Alignment, Font, Border, Fill and Protection are groups that you open and close.
  - Number formats, horizontal and vertical alignment, text direction, line styles and colors are drop-down lists. Each color in a list is shown as a swatch.
  - The font size has − and + buttons. Bold, Italic, Underline and Strikethrough are toolbar buttons.
  - Borders work as in Microsoft Excel: presets (None, Outline, Inside), a button for each edge and diagonal, and a preview cell. Click an edge of the preview to add or remove a line.
- .xlsx: spell checking (Tools > Spelling), with the same panel as for presentations (.pptx).

### Changed

- .pptx: the right pane starts closed when you create a presentation or open a .pptx file.
- .xlsx: the first time you open the right pane in a workbook, it shows the Format Cells tab.
- .xlsx: File > New > New from Template… is now the last item.
- .xlsx: "Open and Repair" is now "Repair Damaged Workbook…". It is unavailable when there is no workbook that failed to open.
- The Settings and About windows open centered on the window they were opened from, on the same display.
- Menus no longer show submenus that have nothing in them. The Format menu's text, alignment, spacing, bullets and borders submenus now have the same commands as the toolbar. In .pptx files, Align, Center on Page, Distribute and Effect Options are shown dimmed when nothing is selected.

## [0.8.0] - 2026-10-12

The first public release.

### Added

- Open, edit and save .docx, .xlsx and .pptx documents, drawn on the GPU to look as they do in Microsoft Office.
- A desktop app for macOS 11 or later (Apple silicon).
- A web version that runs in the browser with WebAssembly and WebGPU, with nothing installed and no files uploaded: [playground](https://ibisjp.github.io/ibis-office/).
- The user interface in English and Japanese.
- Dual licensing: AGPL-3.0-only, or a commercial license from ibis inc.

[Unreleased]: https://github.com/ibisjp/ibis-office/compare/v0.8.5...HEAD
[0.8.5]: https://github.com/ibisjp/ibis-office/compare/v0.8.4...v0.8.5
[0.8.4]: https://github.com/ibisjp/ibis-office/compare/v0.8.3...v0.8.4
[0.8.3]: https://github.com/ibisjp/ibis-office/compare/v0.8.1...v0.8.3
[0.8.1]: https://github.com/ibisjp/ibis-office/compare/v0.8.0...v0.8.1
[0.8.0]: https://github.com/ibisjp/ibis-office/releases/tag/v0.8.0
