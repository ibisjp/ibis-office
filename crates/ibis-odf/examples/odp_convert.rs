// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Converts an OpenDocument presentation to .pptx: `odp_convert <in.odp> <out.pptx>`.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: odp_convert <in.odp> <out.pptx>");
        std::process::exit(2);
    };
    let data = std::fs::read(input).expect("read");
    match ibis_odf::odp_to_pptx(&data) {
        Ok(pptx) => std::fs::write(output, pptx).expect("write"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
