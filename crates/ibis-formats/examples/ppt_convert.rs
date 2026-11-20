// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Write the .pptx an old presentation (.ppt) is read as, for a look at it:
//! `cargo run --release -p ibis-formats --example ppt_convert -- <in.ppt> <out.pptx>`.

fn main() {
    let mut args = std::env::args().skip(1);
    let (input, output) = (args.next().expect("usage: ppt_convert <in.ppt> <out.pptx>"), args.next().expect("usage: ppt_convert <in.ppt> <out.pptx>"));
    let data = std::fs::read(&input).expect("readable file");
    match ibis_formats::ppt_to_pptx(&data) {
        Ok(bytes) => std::fs::write(&output, bytes).expect("writable file"),
        Err(e) => eprintln!("{e}"),
    }
}
