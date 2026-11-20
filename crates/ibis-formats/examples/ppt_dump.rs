// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Print the records of an old presentation (.ppt): `cargo run --release -p ibis-formats --example ppt_dump -- <file.ppt>`.

fn main() {
    let path = std::env::args().nth(1).expect("usage: ppt_dump <file.ppt>");
    let data = std::fs::read(&path).expect("readable file");
    match ibis_formats::ppt_dump(&data) {
        Ok(text) => print!("{text}"),
        Err(e) => eprintln!("{e}"),
    }
}
