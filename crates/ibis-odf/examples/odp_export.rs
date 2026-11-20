// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Writes a presentation file as an OpenDocument presentation: `odp_export <in.pptx> <out.odp>`.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: odp_export <in.pptx> <out.odp>");
        std::process::exit(2);
    };
    let presentation = ibis_odf::read(&std::fs::read(input).expect("read")).expect("parse");
    std::fs::write(output, ibis_odf::write_odp(&presentation).expect("write")).expect("write");
}
