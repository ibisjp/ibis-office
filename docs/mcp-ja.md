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

サーバーは標準入出力で MCP を話します（1 行に JSON のメッセージを 1 つ。ログは標準エラー出力）。プロトコルの版は 2026-07-28 で、`initialize` から始めるクライアントには 2025-11-25 で答えます。通常はネットワークのポートを持たず、MCP のクライアントが起動する手元のプログラムです。`--http <ポート>` を付けると、標準入出力の代わりに `127.0.0.1` で HTTP を待ち受けます（[ChatGPT と接続する](#chatgpt-と接続する)）。`--help` でオプションを表示します。

| オプション | |
|---|---|
| `--allow-dir <フォルダー>` | ファイルを開いて保存してよいフォルダー。何度でも指定できます。**1 つも無ければ、どのファイルにもアクセスできません。** |
| `--read-only` | 読み取りツールだけが利用できます。 |
| `--max-documents N` | 同時に開く文書の数（既定は 8）。 |
| `--max-file-size MB` | 開くファイルの最大の大きさ（単位は MB、既定は 200MB）。zip は大きさの 20 倍まで展開されることがあります。 |
| `--http <ポート>` | 標準入出力の代わりに `http://127.0.0.1:<ポート>/mcp` で待ち受けます。ポートは必須です（既定はありません）。OpenAI の Secure MCP Tunnel 専用です（下記）。 |
| `--allow-origin <オリジン>` | `--http` のとき、このオリジンのウェブページからの呼び出しを受けます。何度でも指定できます。`Origin` ヘッダーの無い呼び出しと、`127.0.0.1`・`localhost` のページからの呼び出しは、いつでも受けます。 |
| `--call <ツール>` | ツールを 1 回だけ呼び、結果を JSON で標準出力に書いて終了します（エラーのときは終了コードが 0 以外で、標準エラー出力に `コード: メッセージ` を書きます）。終了後は何も開いたままになりませんので、`document_id` は次の実行には引き継がれません。`--http` とは併用できません。 |
| `--args <JSON>` | `--call` のとき、ツールの引数を JSON のオブジェクトで指定します（既定は `{}`）。 |
| `--folder <フォルダー>` | `--call` のとき、引数 `folder`（相対パスの基準にする許可フォルダー）を指定します。 |
| `--revision <種類>` | `--call` のとき、引数 `revision.kind` を指定します（`working-tree`・`git`・`open-document`。MCP の値の `_` を `-` にしたものです）。 |
| `--commit <sha>` | `--call` のとき、引数 `revision.commit`（Git のコミットの SHA。16 進数で、ブランチやタグの名前は不可）を指定します。 |
| `--expected-revision <値>` | `--call` のとき、引数 `expected_revision`（変更の元にする状態。保存のときは `file_sha256`）を指定します。文書がその状態でなければ拒否されます。 |

設定する環境変数はありません。

### コマンドラインから 1 つのツールを呼ぶ

`--call` を使うと、MCP のクライアントなしでツールを 1 回呼べます。スクリプトに便利です。`--allow-dir` と `--read-only` の決まりは同じです。

```bash
# ibisOffice のバージョン
ibisOffice --mcp --call app_get_version

# ファイルを開き、その説明（document_id・種類・revision など）を表示する
ibisOffice --mcp --allow-dir ~/Documents --call app_open_document --args '{"path":"report.docx"}'

# 同じファイルの、ある Git のコミットの時点のもの（読み取り専用）
ibisOffice --mcp --allow-dir ~/Documents --call app_open_document --args '{"path":"report.docx"}' --revision git --commit 3f9c2ab
```

`--call` は 1 回ごとに別の実行で、終了後は何も開いたままになりません。`document_id` を取るツール（`docx_get_text` など。引数は `document_id`・`format` など）は、同じサーバーで開いた文書が必要なので、標準入出力の MCP のクライアントから使ってください。

### 読んだ版と、古い状態への編集の拒否

読み取りの結果の `revision` に、読んだ版が入ります。`kind`（`working_tree`・`git`・`open_document`）、Git から読んだときの `commit`、ファイルの `file_sha256` です。開いた文書の状態（`state`）は、編集のたびに変わります。文書を変えるツールとファイルを保存するツールは `expected_revision` を受け付けます。前の結果が返した `state`（`app_save_document` では、読んだファイルの `file_sha256`）を渡すと、文書やファイルがその版でなくなっているときは `revision_mismatch` で拒否され、何も変わりません。読み直して、変更をやり直してください。

## アクセスするフォルダーを許可する

既定では、ファイルにアクセスできません。フォルダーごとに `--allow-dir` で指定します（例：`--allow-dir ~/Documents --allow-dir ~/Work`）。ファイルを開く・書くことができるのは、その実際のパス（リンクをたどり、`..` を解いたもの）が許可したフォルダーの下にあり、拡張子が docx・docm・dotx・xlsx・xlsm・pptx・pptm のときだけです（書き出しは .pdf と .png を書きます）。`--read-only` を付けると、読めるだけで何も変えられません。

## ツール

`app_open_document` でファイルを開くと `document_id` が返り、ほかのツールに渡します。読んで、編集して、`app_save_document` で保存します。長さの単位はポイントです。ツールの 1 回の呼び出しが、元に戻す 1 回分です。

| 区分 | ツール |
|---|---|
| アプリ（全形式） | `app_get_version`、`app_list_documents`、`app_open_document`、`app_get_document_info`、`app_save_document`、`app_close_document`、`app_export_document`（PDF またはページごとの PNG）、`app_undo_document`、`app_redo_document` |
| Word（.docx） | 読む：`docx_get_outline`、`docx_get_text`（Markdown またはプレーンテキスト）、`docx_list_paragraphs`、`docx_get_paragraph`、`docx_list_tables`、`docx_get_table`、`docx_list_styles`、`docx_find_text`。編集：`docx_insert_text`、`docx_delete_text`、`docx_replace_text`、`docx_set_run_format`、`docx_set_paragraph_style`、`docx_insert_table`、`docx_set_table_cell`、`docx_insert_paragraph`、`docx_insert_picture`（PNG・JPEG） |
| Excel（.xlsx） | 読む：`xlsx_list_sheets`（非表示のシート・保護）、`xlsx_get_used_range`（非表示の行・列）、`xlsx_get_cells`（`kind: "format"` で書体・色・罫線・配置）、`xlsx_find_text`、`xlsx_get_sheet_view`、`xlsx_list_names`、`xlsx_list_cell_styles`、`xlsx_list_validations`、`xlsx_list_conditional_formats`、`xlsx_list_tables`（テーブルとオートフィルター）、`xlsx_list_comments`、`xlsx_list_hyperlinks`、`xlsx_get_page_setup`、`xlsx_list_charts`。編集：`xlsx_set_cells`（`=` で始まる文字列は数式）、`xlsx_clear_cells`、`xlsx_set_format`（取り消し線・インデント・文字の回転・縮小・ロック・非表示・セルのスタイルも）、`xlsx_set_borders`、`xlsx_show_sheet`、`xlsx_hide_sheet`、`xlsx_show_rows`、`xlsx_hide_rows`、`xlsx_show_columns`、`xlsx_hide_columns`、`xlsx_set_freeze_panes`、`xlsx_set_sheet_view`、`xlsx_set_page_setup`、`xlsx_set_protection`（パスワードは受けない）、`xlsx_set_calculation`、`xlsx_set_document_properties`、`xlsx_copy_cells`、`xlsx_move_cells`、`xlsx_replace_text`、`xlsx_copy_sheet`、`xlsx_add_name`、`xlsx_delete_name`、`xlsx_set_validation`、`xlsx_add_conditional_format`、`xlsx_delete_conditional_format`、`xlsx_set_auto_filter`、`xlsx_set_table`、`xlsx_set_hyperlink`、`xlsx_add_comment`、`xlsx_delete_comment`、`xlsx_remove_duplicates`、`xlsx_apply_subtotal`、`xlsx_insert_chart`、`xlsx_set_chart`、`xlsx_delete_chart`、`xlsx_add_sheet`、`xlsx_delete_sheet`、`xlsx_rename_sheet`、`xlsx_move_sheet`、`xlsx_insert_rows`、`xlsx_insert_columns`、`xlsx_delete_rows`、`xlsx_delete_columns`、`xlsx_set_size`、`xlsx_merge_cells`、`xlsx_unmerge_cells`、`xlsx_sort_range`、`xlsx_calculate_workbook` |
| PowerPoint（.pptx） | 読む：`pptx_list_slides`、`pptx_get_slide`、`pptx_get_shape_text`、`pptx_get_notes`、`pptx_list_layouts`、`pptx_get_theme`、`pptx_list_comments`、`pptx_get_transition`、`pptx_get_table`、`pptx_list_animations`。編集：`pptx_set_shape_text`、`pptx_replace_text`、`pptx_set_run_format`、`pptx_set_notes`、`pptx_add_slide`、`pptx_delete_slide`、`pptx_move_slide`、`pptx_set_frame`、`pptx_insert_shape`（テキスト ボックス）、`pptx_set_fill`、`pptx_set_line`、`pptx_arrange_shapes`、`pptx_set_shape_properties`、`pptx_set_paragraph_format`、`pptx_set_text_frame`、`pptx_copy_shape`、`pptx_group_shapes`、`pptx_ungroup_shapes`、`pptx_copy_slide`、`pptx_set_slide_properties`、`pptx_set_slide_size`、`pptx_set_transition`、`pptx_delete_animation`、`pptx_set_header_footer`、`pptx_set_document_properties`、`pptx_insert_table`、`pptx_set_table_cell`、`pptx_add_animation`、`pptx_add_comment`、`pptx_align_shapes`、`pptx_insert_chart`、`pptx_set_chart_data`、`pptx_list_charts`、`pptx_insert_connector`（図形の接続点につなぐ。直線・カギ線・曲線）、`pptx_set_slide_properties` はスライドのレイアウトも変える、`pptx_set_theme`、`pptx_insert_picture`（PNG・JPEG）、`pptx_delete_shape` |

ツールはメニューのコマンドと 1 対 1 で対応してはいません。読み取りと、よく使う編集をそろえたもので、形式が違っても同じツール名です。`--read-only` のときは、読み取りツールだけが利用できます。

書式の引数（`bold`・`size`・`color`・`font`・`fill` など）は、省くと変えません。`null` を渡すとその書式を取り除き、スタイルに従う状態に戻します。`font` は、書体の名前、`{ "latin", "east_asian", "complex_script" }`、または `{ "theme": "major" | "minor" }`（テーマの見出し用・本文用の書体。セルや文字には、そのときテーマが示す書体の名前が書かれ、結果の `warnings` に出ます）のどれかです。色は `"#RRGGBB"` または `{ "theme": 名前, "brightness"? }` で、名前には `hyperlink`・`followed_hyperlink` も使えます。テーマ色は docx・xlsx・pptx（文字と塗り）のどれでもテーマへの参照のまま保たれ、テーマを変えると色が追従します。pptx の `brightness` は Office の `TintAndShade` と同じく `lumMod`・`lumOff` で書かれ、読み取りは `{ "theme", "brightness", "resolved" }`（`resolved` は解決した `#RRGGBB`）を返します。

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

ChatGPT は、あなたのコンピューターの上のプログラムに直接はつなげません。OpenAI の **Secure MCP Tunnel** がその間をつなぎます。`tunnel-client` という小さなプログラムをあなたのコンピューターで動かすと、OpenAI へ外向きの HTTPS 接続を作り（インターネットに向けてポートを開けません）、ChatGPT からの呼び出しを ibisOffice に渡します。`tunnel-client` は OpenAI のプログラムで、ibisOffice には**同梱していません**。使い方は OpenAI の案内（[Secure MCP tunnels](https://developers.openai.com/api/docs/guides/secure-mcp-tunnels)）に従ってください。

必要なもの：ChatGPT の **Business** か **Enterprise/Edu** のワークスペースを、**ウェブ版**の ChatGPT で使うこと（カスタム MCP サーバーはモバイルのアプリでは使えません。Pro のアカウントは読み取りだけです）。そして、OpenAI Platform に、組織のトンネルの設定（`tunnel_id`、実行用の API キー、OpenAI が説明する Tunnels の権限）があること。

1. ibisOffice の MCP サーバーを、好きな HTTP のポートと、使ってよいフォルダーを付けて起動します。

   ```
   ibisOffice --mcp --http 8765 --allow-dir ~/Documents
   ```

   （Windows：`ibisOffice-mcp.exe --http 8765 --allow-dir C:\Users\あなた\Documents`）。待ち受けるアドレス `http://127.0.0.1:8765/mcp` が表示されます。使っている間は動かしたままにします。
2. OpenAI の案内に従って `tunnel-client` を起動し、MCP サーバーとしてそのアドレス（`http://127.0.0.1:8765/mcp`）を指定します。
3. ChatGPT（ウェブ版）で chatgpt.com/plugins を開き、カスタム MCP サーバーを追加します。接続は **Tunnel** を選び、自分のトンネルを選びます。最新の手順は OpenAI の [Connect ChatGPT to your MCP server](https://developers.openai.com/plugins/deploy/connect-chatgpt) と [Developer mode and MCP apps in ChatGPT](https://help.openai.com/en/articles/12584461-developer-mode-and-mcp-apps-in-chatgpt) にあります。

HTTP のサーバーの動き：

- MCP の Streamable HTTP の最小の形です。`POST /mcp` で JSON のメッセージを 1 つ受け取り、JSON のメッセージを 1 つ返します（通知には本体のない `202` を返します）。イベントのストリーム、セッション、旧 HTTP+SSE はありません。
- 待ち受けは `127.0.0.1` だけで、TLS も**サインインもありません**。HTTPS はトンネルが運び、誰が呼べるかは OpenAI 側が管理します。このコンピューターであなたとして動いているほかのプログラムも呼べるので、信頼できるコンピューターで、共有してよいフォルダーだけを `--allow-dir` に指定して使い、使い終わったら止めてください。
- `Host` が `127.0.0.1` と `localhost` のものだけを受けます。汎用のトンネル（ngrok など）や、名前であなたのコンピューターに向けられたウェブページからの呼び出しは断ります。ブラウザーのページからの呼び出しは、`127.0.0.1`・`localhost` のページと、`--allow-origin` で指定したオリジンだけを受けます。汎用のトンネル経由で ibisOffice のサーバーを使うことは、サポートしません。
- ツール、フォルダー、`--read-only`、保存の決まりは標準入出力と同じで、**ファイルを変えて保存するツールも使えます**（既定は新しい名前で、置き換えには `overwrite` が要ります）。ChatGPT に読み取りだけをさせるには `--read-only` を付けます。
- 上限：リクエストの本体は 10 MiB まで、1 分に 600 回まで、1 回の答えを待つのは 120 秒まで（トンネルと同じ上限）。`initialize` で 2025-11-25 より古いプロトコルの版を求めると、エラーになります。
- **ChatGPT が読んだ内容は OpenAI に送られます。** ChatGPT がツールを呼ぶと、求められた文書の文章がトンネルを通って OpenAI に届きます（あなたの組織と OpenAI との契約の下で）。ibisOffice 自身は、OpenAI に何も送りません。

## 安全のために

- アクセスできるのは、許可したフォルダーの下のファイルだけです（実際のパスで判断します）。
- ファイルを書くのは `app_save_document` と `app_export_document` だけです。**既定は新しい名前**で、既存のファイルを置き換えるには `overwrite: true` が要ります。開いた後に別のプログラムが変えたファイルは、`overwrite: "force"` のときだけ置き換えます。保存はファイルの隣に一時ファイルを書いて名前を替えるので、失敗しても元のファイルはそのままです。
- マクロは実行せず、ネットワークからは何も取りに行きません（リンクや画像の URL は値であり、要求ではありません）。`--http` のときは `127.0.0.1` への接続を受け付けますが（上記）、外へは何も送りません。
- 文書の文章はデータです。それを含む結果にはその旨が付き、アシスタントは中にある指示に従うべきではありません。
- デジタル署名のあるファイルは、編集を保存すると署名が外れます（結果にその旨が出ます）。
