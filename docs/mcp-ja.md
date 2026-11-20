# AI アシスタントから ibisOffice を使う（MCP）

[English](mcp.md) | **日本語**

ibisOffice は、MCP（Model Context Protocol）のサーバーとして動かせます。Claude などの AI アシスタントが、これを通して Word（.docx）・Excel（.xlsx）・PowerPoint（.pptx）のファイルを開き、文章・セル・表・スライドを読んで書き換え、元に戻し、保存や書き出しをできます。窓は開かず、データはコンピューターの外へ出ません。サーバーはお使いのコンピューターで動き、起動したプログラムとだけ話します。

このページの順序：

1. [サーバーを起動する](#サーバーを起動する)
2. [アクセスするフォルダーを許可する](#アクセスするフォルダーを許可する)
3. [ツール](#ツール)
4. [Claude Desktop と接続する](#claude-desktop-と接続する)
5. [Claude Code と接続する](#claude-code-と接続する)
6. [ChatGPT と接続する](#chatgpt-と接続する)
7. [安全のために](#安全のために)

## サーバーを起動する

| 環境 | コマンド |
|---|---|
| Windows | `ibisOffice-mcp.exe [オプション]`（`ibisOffice.exe` と同じフォルダー。`ibisOffice.exe` は標準入出力を持たない GUI のプログラムです） |
| macOS・Linux | `ibisOffice --mcp [オプション]` |

サーバーは標準入出力で MCP を話します（1 行に JSON のメッセージを 1 つ。ログは標準エラー出力）。プロトコルの版は 2026-07-28 で、`initialize` から始めるクライアントには 2025-11-25 で答えます。HTTP のポートは持ちません。MCP のクライアントが起動する、手元のプログラムです。`--help` でオプションを表示します。

| オプション | |
|---|---|
| `--allow-dir <フォルダー>` | ファイルを開いて保存してよいフォルダー。何度でも指定できます。**1 つも無ければ、どのファイルにもアクセスできません。** |
| `--read-only` | 読み取りツールだけが利用できます。 |
| `--max-documents N` | 同時に開く文書の数（既定は 8）。 |
| `--max-file-size MB` | 開くファイルの最大の大きさ（単位は MB、既定は 200MB）。zip は大きさの 20 倍まで展開されることがあります。 |

設定する環境変数はありません。

## アクセスするフォルダーを許可する

既定では、ファイルにアクセスできません。フォルダーごとに `--allow-dir` で指定します（例：`--allow-dir ~/Documents --allow-dir ~/Work`）。ファイルを開く・書くことができるのは、その実際のパス（リンクをたどり、`..` を解いたもの）が許可したフォルダーの下にあり、拡張子が docx・docm・dotx・xlsx・xlsm・pptx・pptm のときだけです（書き出しは .pdf と .png を書きます）。`--read-only` を付けると、読めるだけで何も変えられません。

## ツール

`app_open_document` でファイルを開くと `document_id` が返り、ほかのツールに渡します。読んで、編集して、`app_save_document` で保存します。長さの単位はポイントです。ツールの 1 回の呼び出しが、元に戻す 1 回分です。

| 区分 | ツール |
|---|---|
| アプリ（全形式） | `app_get_version`、`app_list_documents`、`app_open_document`、`app_get_document_info`、`app_save_document`、`app_close_document`、`app_export_document`（PDF またはページごとの PNG）、`app_undo_document`、`app_redo_document` |
| Word（.docx） | 読む：`docx_get_outline`、`docx_get_text`（Markdown またはプレーンテキスト）、`docx_list_paragraphs`、`docx_get_paragraph`、`docx_list_tables`、`docx_find_text`。編集：`docx_insert_text`、`docx_delete_text`、`docx_replace_text`、`docx_set_run_format`、`docx_insert_table` |
| Excel（.xlsx） | 読む：`xlsx_list_sheets`、`xlsx_get_used_range`、`xlsx_get_cells`、`xlsx_find_text`。編集：`xlsx_set_cells`（`=` で始まる文字列は数式）、`xlsx_clear_cells`、`xlsx_set_format`、`xlsx_add_sheet`、`xlsx_delete_sheet`、`xlsx_rename_sheet`、`xlsx_calculate_workbook` |
| PowerPoint（.pptx） | 読む：`pptx_list_slides`、`pptx_get_slide`、`pptx_get_shape_text`、`pptx_get_notes`、`pptx_list_layouts`。編集：`pptx_set_shape_text`、`pptx_replace_text`、`pptx_set_run_format`、`pptx_set_notes`、`pptx_add_slide`、`pptx_delete_slide`、`pptx_move_slide`、`pptx_set_frame`、`pptx_delete_shape` |

ツールはメニューのコマンドと 1 対 1 で対応してはいません。読み取りと、よく使う編集をそろえたもので、形式が違っても同じツール名です。`--read-only` のときは、読み取りツールだけが利用できます。

## Claude Desktop と接続する

Claude Desktop が、サーバーを自分で起動します。設定ファイルは Claude Desktop から開くか（設定、開発者、設定を編集）、直接編集します。

- Windows：`%APPDATA%\Claude\claude_desktop_config.json`
- macOS：`~/Library/Application Support/Claude/claude_desktop_config.json`

Windows（ibisOffice を入れたフォルダーに読み替えてください。`--mcp` は要りません）：

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

macOS：

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

ファイルに別のサーバーが既にあるときは、`mcpServers` の中にその隣へ `ibisoffice` を足します。パスは絶対パスで書きます。Claude Desktop を完全に終了して、起動し直してください。サーバーは、入力欄のコネクターに出ます。Claude Desktop の MCP のログ（`mcp.log`、`mcp-server-ibisoffice.log`）は、Windows では `%APPDATA%\Claude\logs`、macOS では `~/Library/Logs/Claude` にあります。

出典：[Connect to local MCP servers](https://modelcontextprotocol.io/docs/develop/connect-local-servers)（英語）。

## Claude Code と接続する

Windows：

```
claude mcp add ibisoffice -- "C:\Program Files\ibisOffice\ibisOffice-mcp.exe" --allow-dir "C:\Users\me\Documents"
```

macOS：

```
claude mcp add ibisoffice -- /Applications/ibisOffice.app/Contents/MacOS/ibisOffice --mcp --allow-dir ~/Documents
```

`--` の後ろが、Claude Code が起動するコマンドです。`claude mcp list` で、接続されているかを確かめられます。

## ChatGPT と接続する

現在は、標準入出力（stdio）での対話ですが、近日中に SSE またはストリーミング HTTP に対応する予定です。

## 安全のために

- アクセスできるのは、許可したフォルダーの下のファイルだけです（実際のパスで判断します）。
- ファイルを書くのは `app_save_document` と `app_export_document` だけです。**既定は新しい名前**で、既存のファイルを置き換えるには `overwrite: true` が要ります。開いた後に別のプログラムが変えたファイルは、`overwrite: "force"` のときだけ置き換えます。保存はファイルの隣に一時ファイルを書いて名前を替えるので、失敗しても元のファイルはそのままです。
- マクロは実行せず、ネットワークからは何も取りに行きません（リンクや画像の URL は値であり、要求ではありません）。
- 文書の文章はデータです。それを含む結果にはその旨が付き、アシスタントは中にある指示に従うべきではありません。
- デジタル署名のあるファイルは、編集を保存すると署名が外れます（結果にその旨が出ます）。
