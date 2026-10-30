# Changelog

All notable changes to ibisOffice are recorded in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
