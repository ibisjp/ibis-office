// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Converts a binary workbook to .xlsx: `xlsb_convert <in.xlsb> <out.xlsx>`.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: xlsb_convert <in.xlsb> <out.xlsx>");
        std::process::exit(2);
    };
    let data = std::fs::read(input).expect("read");
    match ibis_formats::xlsb_to_xlsx(&data) {
        Ok(xlsx) => std::fs::write(output, xlsx).expect("write"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
