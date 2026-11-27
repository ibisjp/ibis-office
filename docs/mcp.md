# Using ibisOffice from an AI assistant (MCP)

**English** | [日本語](mcp-ja.md)

ibisOffice can run as an MCP (Model Context Protocol) server. An AI assistant such as Claude opens Word (.docx), Excel (.xlsx) and PowerPoint (.pptx) files through it, reads and edits their text, cells, tables and slides, undoes edits, and saves or exports the result. No window is opened and nothing leaves your computer: the server runs on your machine and speaks only to the program that started it (or, with `--http`, to programs on your computer through port 127.0.0.1).

This page, in order:

1. [Starting the server](#starting-the-server)
2. [Allowing access to your folders](#allowing-access-to-your-folders)
3. [Tools](#tools)
4. [Connecting Claude Desktop](#connecting-claude-desktop)
5. [Connecting Claude Code](#connecting-claude-code)
6. [Connecting ChatGPT](#connecting-chatgpt)
7. [Safety](#safety)

## Starting the server

| Platform | Command |
|---|---|
| Windows | `ibisOffice-mcp.exe [options]` (next to `ibisOffice.exe`, which is a GUI program with no standard input and output) |
| macOS, Linux | `ibisOffice --mcp [options]` |

The server speaks the Model Context Protocol on standard input and output, one JSON message per line (the log goes to standard error). It uses protocol version 2026-07-28, and 2025-11-25 for a client that starts with `initialize`. By default it has no network port: it is a local program that an MCP client starts. With `--http <port>` it instead listens on `127.0.0.1` for HTTP (see [Connecting ChatGPT](#connecting-chatgpt)). `--help` prints the options.

| Option | |
|---|---|
| `--allow-dir <folder>` | A folder whose files may be opened and saved. Repeatable. **With none, no file can be accessed.** |
| `--read-only` | Only the read tools are available. |
| `--max-documents N` | Documents open at once (default 8). |
| `--max-file-size MB` | The largest file that is opened, in MB (default 200 MB). A zip may expand to 20 times its size. |
| `--http <port>` | Listen on `http://127.0.0.1:<port>/mcp` instead of standard input and output. The port is required (there is no default). For OpenAI's Secure MCP Tunnel only; see below. |
| `--allow-origin <origin>` | With `--http`: a web page of this origin may call the server. Repeatable. Calls with no `Origin` header, and pages of `127.0.0.1` and `localhost`, are always accepted. |
| `--call <tool>` | Call one tool once, print its result as JSON on standard output, and exit (an error gives a non-zero exit code and `code: message` on standard error). Nothing stays open afterward, so a `document_id` does not reach the next run. Cannot be combined with `--http`. |
| `--args <JSON>` | With `--call`: the tool's arguments as a JSON object (default `{}`). |
| `--folder <folder>` | With `--call`: the argument `folder`, the allowed folder a relative path is from. |
| `--revision <kind>` | With `--call`: the argument `revision.kind` (`working-tree`, `git` or `open-document`; the MCP values with `-` for `_`). |
| `--commit <sha>` | With `--call`: the argument `revision.commit`, the SHA of a Git commit (hexadecimal; not a branch or tag name). |
| `--expected-revision <value>` | With `--call`: the argument `expected_revision`, the state (or, for a save, the `file_sha256`) the change is made on. It is refused when the document is not at it. |

There are no environment variables to set.

### Calling one tool from the command line

`--call` runs a single tool without an MCP client, which is handy in scripts. The same `--allow-dir` and `--read-only` rules apply.

```bash
# The version of ibisOffice
ibisOffice --mcp --call app_get_version

# Open a file and print its description (document_id, kind, revision, ...)
ibisOffice --mcp --allow-dir ~/Documents --call app_open_document --args '{"path":"report.docx"}'

# The same file as it was in a Git commit (read only)
ibisOffice --mcp --allow-dir ~/Documents --call app_open_document --args '{"path":"report.docx"}' --revision git --commit 3f9c2ab
```

Each `--call` is a separate run and nothing stays open afterward. Tools that take a `document_id` (such as `docx_get_text`, whose arguments are `document_id`, `format` and so on) need a document opened in the same server, so use them through an MCP client over standard input and output.

### Which version was read, and refusing a stale edit

A read result says which version was read in `revision`: the `kind` (`working_tree`, `git` or `open_document`), the `commit` for a Git read, and the `file_sha256` of the file. The state of an open document changes with each edit. A tool that changes a document or saves a file accepts `expected_revision`: the `state` an earlier result gave (or, for `app_save_document`, the `file_sha256` of the file you read). When the document or the file is no longer at that version, the call is refused with `revision_mismatch` and nothing is changed; read it again and redo the change.

## Allowing access to your folders

By default, no file can be accessed. Give each folder with `--allow-dir`, for example `--allow-dir ~/Documents --allow-dir ~/Work`. A file can be opened or written only when its real path (links followed, `..` resolved) is under one of these folders and its extension is docx, docm, dotx, xlsx, xlsm, pptx or pptm (an export writes .pdf or .png). Add `--read-only` to let the assistant read but not change anything.

## Tools

Open a file with `app_open_document`. It returns a `document_id` that goes into the other tools. Read, edit, then save with `app_save_document`. Lengths are in points. One tool call is one step of undo.

| Group | Tools |
|---|---|
| App (all formats) | `app_get_version`, `app_list_documents`, `app_open_document`, `app_get_document_info`, `app_save_document`, `app_close_document`, `app_export_document` (PDF or PNG pages), `app_undo_document`, `app_redo_document` |
| Word (.docx) | Read: `docx_get_outline`, `docx_get_text` (Markdown or plain text), `docx_list_paragraphs`, `docx_get_paragraph`, `docx_list_tables`, `docx_get_table`, `docx_list_styles` (with each style's own format), `docx_find_text`, `docx_get_page_setup` (per section: paper, margins, columns, text direction, grid, headers and footers; watermark and page color), `docx_list_comments`, `docx_list_revisions`, `docx_list_shapes`, `docx_list_content_controls`; `docx_get_paragraph` also says the paragraph's format, list, links, fields, bookmarks and comments, and `app_get_document_info` the document properties. Edit: `docx_insert_text`, `docx_delete_text`, `docx_replace_text`, `docx_set_run_format`, `docx_set_paragraph_style`, `docx_insert_table`, `docx_set_table_cell`, `docx_insert_paragraph`, `docx_insert_picture` (PNG or JPEG), `docx_set_paragraph_format`, `docx_set_page_setup` (the last section), `docx_set_hyperlink`, `docx_set_track_changes`, `docx_set_document_properties`, `docx_set_page_color`, `docx_set_numbering`, `docx_insert_table_rows`, `docx_delete_table_rows`, `docx_insert_table_columns`, `docx_delete_table_columns`, `docx_delete_table`, `docx_set_table_format`, `docx_insert_page_break`, `docx_insert_page_number`, `docx_insert_field`, `docx_insert_content_control`, `docx_set_header`, `docx_set_footer`, `docx_add_comment`, `docx_delete_comment`, `docx_apply_revisions` (accept or reject), `docx_merge_table_cells`, `docx_split_table_cell`, `docx_insert_text_box`, `docx_insert_shape`, `docx_set_shape_properties`, `docx_set_picture_properties`, `docx_set_watermark`, `docx_list_charts`, `docx_insert_chart` (with `data`, `series_in`, `title`, `legend`), `docx_set_chart_data`, `docx_set_table_of_contents` (no page numbers: Word and the window make them when the file is opened), `docx_set_picture_wrap` |
| Excel (.xlsx) | Read: `xlsx_list_sheets` (hidden sheets, protection), `xlsx_get_used_range` (hidden rows and columns), `xlsx_get_cells` (`kind: "format"` gives fonts, colours, borders, alignment), `xlsx_find_text`, `xlsx_get_sheet_view`, `xlsx_list_names`, `xlsx_list_cell_styles`, `xlsx_list_validations`, `xlsx_list_conditional_formats`, `xlsx_list_tables` (tables and the auto filter), `xlsx_list_comments`, `xlsx_list_hyperlinks`, `xlsx_get_page_setup`, `xlsx_list_charts`. Edit: `xlsx_set_cells` (a string starting with `=` is a formula), `xlsx_clear_cells`, `xlsx_set_format` (also strike, indent, text_rotation, shrink_to_fit, locked, hidden, style), `xlsx_set_borders`, `xlsx_show_sheet`, `xlsx_hide_sheet`, `xlsx_show_rows`, `xlsx_hide_rows`, `xlsx_show_columns`, `xlsx_hide_columns`, `xlsx_set_freeze_panes`, `xlsx_set_sheet_view`, `xlsx_set_page_setup`, `xlsx_set_protection` (no password is taken), `xlsx_set_calculation`, `xlsx_set_document_properties`, `xlsx_copy_cells`, `xlsx_move_cells`, `xlsx_replace_text`, `xlsx_copy_sheet`, `xlsx_add_name`, `xlsx_delete_name`, `xlsx_set_validation`, `xlsx_add_conditional_format`, `xlsx_delete_conditional_format`, `xlsx_set_auto_filter`, `xlsx_set_table`, `xlsx_set_hyperlink`, `xlsx_add_comment`, `xlsx_delete_comment`, `xlsx_remove_duplicates`, `xlsx_apply_subtotal`, `xlsx_insert_chart`, `xlsx_set_chart`, `xlsx_delete_chart`, `xlsx_add_sheet`, `xlsx_delete_sheet`, `xlsx_rename_sheet`, `xlsx_move_sheet`, `xlsx_insert_rows`, `xlsx_insert_columns`, `xlsx_delete_rows`, `xlsx_delete_columns`, `xlsx_set_size`, `xlsx_merge_cells`, `xlsx_unmerge_cells`, `xlsx_sort_range`, `xlsx_calculate_workbook` |
| PowerPoint (.pptx) | Read: `pptx_list_slides`, `pptx_get_slide`, `pptx_get_shape_text`, `pptx_get_notes`, `pptx_list_layouts`, `pptx_get_theme`, `pptx_list_comments`, `pptx_get_transition`, `pptx_get_table`, `pptx_list_animations`. Edit: `pptx_set_shape_text`, `pptx_replace_text`, `pptx_set_run_format`, `pptx_set_notes`, `pptx_add_slide`, `pptx_delete_slide`, `pptx_move_slide`, `pptx_set_frame`, `pptx_insert_shape` (a text box), `pptx_set_fill`, `pptx_set_line`, `pptx_arrange_shapes`, `pptx_set_shape_properties`, `pptx_set_paragraph_format`, `pptx_set_text_frame`, `pptx_copy_shape`, `pptx_group_shapes`, `pptx_ungroup_shapes`, `pptx_copy_slide`, `pptx_set_slide_properties`, `pptx_set_slide_size`, `pptx_set_transition`, `pptx_delete_animation`, `pptx_set_header_footer`, `pptx_set_document_properties`, `pptx_insert_table`, `pptx_set_table_cell`, `pptx_add_animation`, `pptx_add_comment`, `pptx_align_shapes`, `pptx_insert_chart`, `pptx_set_chart_data`, `pptx_list_charts`, `pptx_insert_connector` (glued to the shapes' connection sites; a straight, elbow or curved line), `pptx_set_slide_properties` also changes the slide's layout, `pptx_set_theme`, `pptx_insert_picture` (PNG or JPEG), `pptx_delete_shape` |

The tools do not correspond one to one to the menu commands: they cover reading and the common edits, with the same tool names whatever the format. With `--read-only`, only the read tools are available.

Format arguments (`bold`, `size`, `color`, `font`, `fill` ...) that are left out are not changed. `null` takes the format away so that it follows the styles again. A font is a name, `{ "latin", "east_asian", "complex_script" }`, or `{ "theme": "major" | "minor" }` (the theme's heading or body typeface; cells and text keep the name the theme gives now, and the result says so in `warnings`). A colour is `"#RRGGBB"` or `{ "theme": name, "brightness"? }`; the names include `hyperlink` and `followed_hyperlink`. A theme colour stays a reference to the theme in a docx, an xlsx and a pptx (text and fill): changing the theme changes the colour. In a pptx, `brightness` is written as `lumMod`/`lumOff` as Office's `TintAndShade` does, and reads return `{ "theme", "brightness", "resolved" }` (`resolved` is the `#RRGGBB` it works out to).

## Connecting Claude Desktop

Claude Desktop starts the server itself. Open the configuration file from Claude Desktop (Settings, Developer, Edit Config), or edit it directly:

- Windows: `%APPDATA%\Claude\claude_desktop_config.json`
- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`

Windows (use the folder ibisOffice is installed in; `--mcp` is not needed):

```json
{
  "mcpServers": {
    "ibisoffice": {
      "command": "C:\\Program Files\\ibisOffice\\ibisOffice-mcp.exe",
      "args": ["--allow-dir", "C:\\Users\\me\\Documents"]
    }
  }
}
```

macOS:

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

If the file already has other servers, add `ibisoffice` next to them under `mcpServers`. Paths must be absolute. Quit Claude Desktop completely and start it again. The server then appears under the connectors of the chat box. Claude Desktop writes its MCP log (`mcp.log` and `mcp-server-ibisoffice.log`) in `%APPDATA%\Claude\logs` on Windows and `~/Library/Logs/Claude` on macOS.

Source: [Connect to local MCP servers](https://modelcontextprotocol.io/docs/develop/connect-local-servers).

## Connecting Claude Code

Windows:

```
claude mcp add ibisoffice -- "C:\Program Files\ibisOffice\ibisOffice-mcp.exe" --allow-dir "C:\Users\me\Documents"
```

macOS:

```
claude mcp add ibisoffice -- /Applications/ibisOffice.app/Contents/MacOS/ibisOffice --mcp --allow-dir ~/Documents
```

Everything after `--` is the command that Claude Code starts. `claude mcp list` shows whether it is connected.

## Connecting ChatGPT

ChatGPT cannot reach a program on your computer directly. OpenAI's **Secure MCP Tunnel** connects them: a small program, `tunnel-client`, runs on your computer, makes an outgoing HTTPS connection to OpenAI (no port on your computer is opened to the internet), and passes ChatGPT's calls to ibisOffice. `tunnel-client` is OpenAI's program and is **not** included with ibisOffice; follow OpenAI's own guide for it ([Secure MCP tunnels](https://developers.openai.com/api/docs/guides/secure-mcp-tunnels)).

What you need: a ChatGPT **Business** or **Enterprise/Edu** workspace, used in the **web** version of ChatGPT (custom MCP servers are not available in the mobile apps, and a Pro account can only read), and a tunnel set up in the OpenAI Platform for your organization (a `tunnel_id` and a runtime API key, and the Tunnels permissions OpenAI describes).

1. Start ibisOffice's MCP server with an HTTP port of your choice and the folders it may use:

   ```
   ibisOffice --mcp --http 8765 --allow-dir ~/Documents
   ```

   (Windows: `ibisOffice-mcp.exe --http 8765 --allow-dir C:\Users\you\Documents`.) It prints the address it listens on, `http://127.0.0.1:8765/mcp`. Keep it running while you use it.
2. Start `tunnel-client` as OpenAI describes, with that address as the MCP server (`http://127.0.0.1:8765/mcp`).
3. In ChatGPT (web), open chatgpt.com/plugins, add a custom MCP server, choose **Tunnel** as the connection, and pick your tunnel. OpenAI's pages, [Connect ChatGPT to your MCP server](https://developers.openai.com/plugins/deploy/connect-chatgpt) and [Developer mode and MCP apps in ChatGPT](https://help.openai.com/en/articles/12584461-developer-mode-and-mcp-apps-in-chatgpt), have the current steps.

How the HTTP server behaves:

- It is MCP's Streamable HTTP in its smallest form: `POST /mcp` with one JSON message, answered with one JSON message (a notification is answered with `202` and no body). There is no event stream, no session, and no older HTTP+SSE.
- It listens on `127.0.0.1` only and has no TLS and **no sign-in**: the tunnel carries the HTTPS and OpenAI's side controls who can call it. Anything else that runs on your computer as you can also call it, so use it only on a computer you trust, with `--allow-dir` set to the folders you mean to share, and stop it when you are done.
- Only the `Host` names `127.0.0.1` and `localhost` are accepted, so a general-purpose tunnel (ngrok and the like) or a web page that was pointed at your computer by name is refused. Calls from a browser page are accepted only from `127.0.0.1` and `localhost` pages and from origins you give with `--allow-origin`. Using ibisOffice's server through a general tunnel is not supported.
- The tools, the folders, `--read-only` and the rules for saving are the same as over standard input and output, **including the tools that change and save files** (a new name by default; replacing a file needs `overwrite`). Use `--read-only` to let ChatGPT only read.
- Limits: a request body up to 10 MiB, 600 requests a minute, and 120 seconds for an answer (the tunnel's own limits). An `initialize` that asks for a protocol version older than 2025-11-25 is an error.
- **What ChatGPT reads is sent to OpenAI.** When ChatGPT calls a tool, the text of the documents it asks for travels back through the tunnel to OpenAI, under your organization's terms with OpenAI. ibisOffice itself sends nothing to OpenAI.

## Safety

- Only the files under the allowed folders can be accessed, by their real paths.
- Files are written only by `app_save_document` and `app_export_document`. **A new name is the default**; replacing a file needs `overwrite: true`, and a file changed by another program since it was opened is replaced only with `overwrite: "force"`. A save writes a temporary file next to the file and renames it, so a failure leaves the old file as it was.
- Macros are not run, and nothing is fetched from the network (links and picture URLs are values, not requests). With `--http` the server accepts connections on `127.0.0.1` (see above); it still sends nothing out.
- The text of a document is data. A result that holds it says so, and the assistant should not follow instructions that are in it.
- A digitally signed file loses its signature when an edit is saved (the result says so).
