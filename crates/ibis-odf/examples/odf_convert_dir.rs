// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Converts every OpenDocument file of a folder to the OOXML file it is read as: `odf_convert_dir <in dir> <out dir>`
//! (`.odt` -> `.docx`, `.ods` -> `.xlsx`, `.odp` -> `.pptx`). A quick way to try the readers without building the app.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: odf_convert_dir <in dir> <out dir>");
        std::process::exit(2);
    };
    std::fs::create_dir_all(output).expect("out dir");
    let mut failed = 0;
    for entry in std::fs::read_dir(input).expect("in dir").flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let target = match ext.as_str() {
            "odt" => "docx",
            "ods" => "xlsx",
            "odp" => "pptx",
            _ => continue,
        };
        let data = std::fs::read(&path).expect("read");
        match ibis_odf::to_ooxml(&data) {
            Ok(Some(bytes)) => std::fs::write(std::path::Path::new(output).join(path.file_stem().unwrap()).with_extension(target), bytes).expect("write"),
            Ok(None) => eprintln!("{}: not an OpenDocument file", path.display()),
            Err(e) => {
                failed += 1;
                eprintln!("{}: {e}", path.display());
            }
        }
    }
    if failed > 0 {
        std::process::exit(1);
    }
}
