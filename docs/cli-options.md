<p align="right"><b>English</b> | <a href="cli-options-ja.md">日本語</a></p>

# ibisOffice Command-Line Options

ibisOffice is not only a graphical desktop office suite, but also features a powerful command-line interface (CLI) suitable for terminals, automated shell scripts, batch processing, and CI/CD pipelines.

You can perform **headless exports** to convert Word, Excel, and PowerPoint documents into PDF or PNG images at high speed without opening a window, customize print/export settings, and benchmark launch and rendering performance.

---

## Table of Contents

1. [Basic Syntax](#basic-syntax)
2. [File Opening & Display Options](#file-opening--display-options)
3. [Headless Export (--export)](#headless-export---export)
4. [Print & Export Options](#print--export-options)
   - [Word (.docx) Options](#word-docx-options)
   - [PowerPoint (.pptx) Options](#powerpoint-pptx-options)
5. [UI Snapshot (--export ui)](#ui-snapshot---export-ui)
6. [Performance Benchmarking (--timing)](#performance-benchmarking---timing)
7. [Cookbook & Practical Examples](#cookbook--practical-examples)

---

## Basic Syntax

```bash
# Standard GUI launch (with a file or the start window)
ibisOffice [OPTIONS] [FILE]

# Headless export (render and save files without opening a window)
ibisOffice --export <format> [EXPORT_OPTIONS] <FILE>

# Headless UI snapshot (capture editor UI states to PNG)
ibisOffice --export ui [--out-dir <dir>] [--steps <steps>] [<FILE>]

# Performance and startup benchmark
ibisOffice [--timing] [--quit-after-first-frame] [<FILE>]
```

### Supported File Formats
- **Word Documents**: `.docx`
- **Excel Workbooks**: `.xlsx`
- **PowerPoint Presentations**: `.pptx`

*Note: If `FILE` is omitted, ibisOffice opens the Start Window with templates and blank document creation options.*

---

## File Opening & Display Options

| Option | Argument | Description |
|---|---|---|
| `FILE` | `<path>` | Path to the `.docx`, `.xlsx`, or `.pptx` document to open. |
| `--slide` | `<number>` | Open the presentation directly at the specified slide number (1-based index). |
| `--read-only` | (none) | Open the file for viewing only: editing is turned off and the side panel stays closed. |

**Example:**
```bash
# Open presentation directly jumping to slide 5
ibisOffice --slide 5 presentation.pptx

# Open a document for viewing only
ibisOffice --read-only report.docx
```

---

## Headless Export (--export)

Runs completely in the background without opening any GUI window. Documents are rendered via GPU/CPU acceleration and exported to the target directory, after which the application immediately terminates. Ideal for CI/CD environments and automated batch workflows.

| Option | Argument | Description |
|---|---|---|
| `--export` | `<format>` | Export format (required).<br>• `pdf`: High-quality vector PDF with embedded fonts and graphics.<br>• `png`: Raster image sequence for each page or slide.<br>• `pdf-image`: PDF containing full-page rasterized images.<br>• `ui`: Snapshot of the editor UI window. |
| `--out-dir` | `<dir>` | Target output directory (default: current directory `.`). Created automatically if it does not exist. |
| `--width` | `<px>` | Width of PNG export in pixels (default: `1920`). Aspect ratio is preserved. |
| `--range` | `<range>` | Slide or page range to export (e.g., `1`, `1,3-5`, `2-`). The old name `--slides` still works but prints a warning. Exports all pages/slides if omitted. |
| `--include-layout` | (none) | Include layout information when exporting slides. The old name `--layout` still works but prints a warning. |

**Examples:**
```bash
# Convert entire presentation to PDF
ibisOffice --export pdf presentation.pptx

# Export pages 1 to 3 as 2560px wide PNG images into dist/
ibisOffice --export png --out-dir dist/ --width 2560 --range 1-3 document.docx
```

---

## Print & Export Options

Provides fine-grained output control fully compatible with Microsoft Office print and export specifications.

### Word (.docx) Options

| Option | Description |
|---|---|
| `--print-backgrounds` | Include background colors and background images in the output (Word: "Print background colors and images"). |
| `--print-hidden-text` | Include hidden text in the output (Word: "Print hidden text"). |
| `--no-print-markup` | Exclude revision marks and comments from the output (equivalent to disabling "Print Markup"). |

**Example:**
```bash
# Generate clean PDF with backgrounds but without revision markup
ibisOffice --export pdf --print-backgrounds --no-print-markup report.docx
```

### PowerPoint (.pptx) Options

#### Color Mode
| Option | Argument | Description |
|---|---|---|
| `--print-color` | `color` \| `gray` \| `bw` | Output color mode (default: `color`).<br>• `color`: Full color.<br>• `gray`: Grayscale.<br>• `bw`: High-contrast black and white. |

#### Handout Settings
| Option | Argument | Description |
|---|---|---|
| `--handout` | `<n>` | Export as a handout with multiple slides per page.<br>Supported values: `1`, `2`, `3`, `4`, `6`, `8`, `9`, `16`. |
| `--handout-landscape` | (none) | Render handout pages in landscape orientation. |
| `--handout-order` | `horizontal` \| `vertical` | The order of the slides on a handout page (default: `horizontal`, as PowerPoint). `horizontal` fills a row before the next; `vertical` fills a column before the next (4, 6, 9 to a page). |
| `--print-frame` | (none) | Draw a thin frame border around each slide. |
| `--notes-pages` | (none) | Export as Notes Pages (speaker notes placed beneath each slide). |
| `--outline-pages` | (none) | Export presentation text outline only. |

#### Handout Header & Footer
| Option | Argument | Description |
|---|---|---|
| `--handout-header` | `<text>` | Text for the handout header. |
| `--handout-date` | `<text>` | Text for the handout date (default: today's date, unless the handout master's header/footer settings turn the date off). |
| `--handout-footer` | `<text>` | Text for the handout footer. |
| `--handout-number` | (none) | Print page numbers on handout pages (on by default, unless the handout master's header/footer settings turn them off; this option prints them anyway). |
| `--no-handout-date` | (none) | Print no date on handout, notes and outline pages. |
| `--no-handout-number` | (none) | Print no page number on handout, notes and outline pages. |

**Examples:**
```bash
# 3-slides-per-page landscape handout PDF with frames, header, date, and page numbers
ibisOffice --export pdf --handout 3 --handout-landscape --print-frame \
  --handout-header "Quarterly Strategy Review" --handout-date "2026-10-16" --handout-number \
  presentation.pptx

# Export speaker notes pages to PDF
ibisOffice --export pdf --notes-pages presentation.pptx
```

---

## UI Snapshot (--export ui)

Takes headless snapshots of the ibisOffice editor window itself. Useful for UI regression testing, design verification, and documentation generation.

| Option | Argument | Description |
|---|---|---|
| `--steps` | `<steps>` | Comma-separated list of UI steps to execute (default: `start`).<br>Example: `start,pagedown,end` |

**Example:**
```bash
# Capture the editor start window into output/
ibisOffice --export ui --out-dir output/

# Capture editor state when document is loaded
ibisOffice --export ui --out-dir output/ presentation.pptx
```

---

## Performance Benchmarking (--timing)

Enables precise instrumentation for measuring launch latency, document parsing, layout calculation, and GPU rendering stages.

| Option | Description |
|---|---|
| `--timing` | Prints elapsed time in milliseconds for each startup stage to standard error (stderr) in `ibisOffice timing: <stage> <ms>` format. |
| `--quit-after-first-frame` | Automatically exits the application after the window and document have completed initial rendering (post second-frame stabilization). Ideal for repeated startup benchmarks. |

### Measured Stages
- `main`: Execution of the `main()` entrypoint.
- `app`: Application event loop and platform runtime initialized.
- `window`: Native window created.
- `first-frame`: Initial window frame rendered.
- `second-frame`: Second frame rendered.
- `read`: File loaded from disk (asynchronous worker thread).
- `parsed`: OOXML document structure parsed into memory model.
- `loaded`: Layout computed and document mounted in window.
- `document-frame`: Document content initially drawn.
- `document-second-frame`: Document content stable draw complete.
- `slide-pictures`: Pictures for active slide decoded.
- `all-pictures`: All document pictures decoded and thumbnails rendered.

**Example:**
```bash
# Benchmark document launch time and exit immediately after rendering
ibisOffice --timing --quit-after-first-frame presentation.pptx
```

Sample output:
```text
ibisOffice timing: main 0.12
ibisOffice timing: app 12.45
ibisOffice timing: window 28.30
ibisOffice timing: read 31.10
ibisOffice timing: parsed 38.50
ibisOffice timing: loaded 45.20
ibisOffice timing: first-frame 48.90
ibisOffice timing: document-frame 52.10
ibisOffice timing: document-second-frame 68.40
```

---

## Cookbook & Practical Examples

### 1. Batch Convert Documents to PDF (CI/CD / Shell)
```bash
for file in docs/*.docx slides/*.pptx; do
  ibisOffice --export pdf --out-dir dist/ "$file"
done
```

### 2. Generate High-Resolution Web Thumbnail
```bash
# Export slide 1 as 1920px PNG into thumbnails/
ibisOffice --export png --range 1 --width 1920 --out-dir thumbnails/ presentation.pptx
```

### 3. Generate 2-in-1 Grayscale Meeting Handouts
```bash
ibisOffice --export pdf --handout 2 --print-color bw --print-frame \
  --handout-header "Board of Directors Meeting" \
  --handout-footer "CONFIDENTIAL" \
  --handout-number --out-dir handouts/ agenda.pptx
```
