# ibis-mcp

The MCP (Model Context Protocol) server of ibisOffice: an AI client (Claude Desktop, Claude Code, ...) reads, edits and saves docx, xlsx and pptx files through tools. No window is opened and nothing leaves the computer.

## Starting it

- macOS and Linux: `ibisOffice --mcp [options]`
- Windows: `ibisOffice-mcp.exe [options]` (next to `ibisOffice.exe`, which is a GUI program with no standard input and output)

| Option | |
|---|---|
| `--allow-dir <folder>` | A folder whose files may be opened and saved. Repeatable. **With none, no file can be reached.** |
| `--read-only` | Offer only the tools that read. |
| `--max-documents N` | Documents open at once (default 8). |
| `--max-file-size MB` | The largest file that is opened (default 200). A zip may expand to 20 times its size. |

The server speaks the Model Context Protocol 2026-07-28, and 2025-11-25 to a client that starts with `initialize`, one JSON message a line on standard input and output (the log is standard error).

## Claude Desktop

`claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "ibisoffice": {
      "command": "/Applications/ibisOffice.app/Contents/MacOS/ibisOffice",
      "args": ["--mcp", "--allow-dir", "/Users/me/Documents"]
    }
  }
}
```

On Windows, `"command": "C:\\Program Files\\ibisOffice\\ibisOffice-mcp.exe"` and `"args": ["--allow-dir", "C:\\Users\\me\\Documents"]`. Claude Code: `claude mcp add ibisoffice -- /Applications/ibisOffice.app/Contents/MacOS/ibisOffice --mcp --allow-dir ~/Documents`.

PowerShell shows and sends text in the console's code page unless it is told otherwise. To try the server by hand there, set UTF-8 first (a client such as Claude Desktop needs nothing): `[Console]::OutputEncoding = [Text.UTF8Encoding]::new(); $OutputEncoding = [Text.UTF8Encoding]::new()`.

## What it does

Open a file with `app_open_document` (the `document_id` it returns goes into the other tools), read it (`docx_get_text`, `xlsx_get_cells`, `pptx_get_slide`, ...), edit it (`docx_replace_text`, `xlsx_set_cells`, `pptx_set_frame`, `docx_set_run_format`, ...), and save it with `app_save_document`. Lengths are points. One tool call is one step of undo (`app_undo_document`).

## Safety

- Only the files under the allowed folders, by their real paths (links followed), with a docx, docm, dotx, xlsx, xlsm, pptx or pptm extension (an export writes .pdf or .png).
- The files are written only by `app_save_document` and `app_export_document`. **A new name is the default**; replacing a file needs `overwrite: true`, and a file changed by another program since it was opened is replaced only with `overwrite: "force"`. A save writes a temporary file next to the file and renames it, so a failure leaves the old file as it was.
- Macros are not run, and nothing is fetched from the network (links and picture URLs are values, not requests).
- The text of a document is data. A result that holds it says so in its `_meta`; a client should not follow instructions that are in it.
- A digitally signed file loses its signature when an edit is saved (the result says so).
