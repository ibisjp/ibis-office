// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ibis-office-roundtrip: open an Office file as ibisOffice does and save it
//! again, with no edit, for checking that saving does not break files.
//!
//!   ibis-office-roundtrip keep|rewrite <in.pptx|xlsx|docx> <out>
//!
//! - `keep`: the save ibisOffice does (parts with no change are written back
//!   as they were).
//! - `rewrite`: every XML part written again from the XML tree
//!   (`SaveOptions::regenerate_xml`), to check the writer on all of them.
//!
//! Exit status: 0 when saved, 2 on errors.

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [mode, input, output] = &args[..] else {
        eprintln!("使い方: ibis-office-roundtrip keep|rewrite <入力> <出力>");
        return std::process::ExitCode::from(2);
    };
    let regenerate_xml = match mode.as_str() {
        "keep" => false,
        "rewrite" => true,
        other => {
            eprintln!("ibis-office-roundtrip: keep か rewrite を指定してください: {other}");
            return std::process::ExitCode::from(2);
        }
    };
    let result = (|| -> Result<(), String> {
        let data = std::fs::read(input).map_err(|e| format!("{input}: {e}"))?;
        let opened = ibis_formats::open(data).map_err(|e| format!("{input}: {e}"))?;
        let saved = opened.source.save(&opened.presentation, ibis_formats::SaveOptions { regenerate_xml }).map_err(|e| format!("{input}: {e}"))?;
        std::fs::write(output, saved).map_err(|e| format!("{output}: {e}"))
    })();
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("ibis-office-roundtrip: {err}");
            std::process::ExitCode::from(2)
        }
    }
}
