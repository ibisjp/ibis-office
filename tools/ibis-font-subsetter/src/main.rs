// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ibis-font-subsetter --font <file> --id <id> [--name <shown name>] --typeface <name>... --license <file> --out <directory> [--chunk <size>] [--hinting keep|remove]
//!
//! Cuts an OFL font into the blocks of the browser version (see the library). Writes, into `<directory>/<id>/`: the blocks (`base.<hash>.ttf`,
//! `0001.<hash>.ttf`, ...), `blocks.json` (what `scripts/web-loader-setup.mjs --subset` puts in `loader-manifest.json`), the font's licence as `OFL.txt` and a
//! `README.md` (where the blocks come from and what was changed). `--chunk` is the limit of a block's deflated size (default 512KiB; `KiB` or `MiB`).

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use ibis_font_subsetter::{Hinting, Options, cut};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Serialize)]
struct Fragment {
    id: String,
    typeface: Vec<String>,
    license: &'static str,
    #[serde(rename = "licenseFile")]
    license_file: String,
    source: Source,
    hinting: &'static str,
    chunk: usize,
    blocks: Vec<FragmentBlock>,
}

#[derive(Serialize)]
struct Source {
    file: String,
    size: usize,
    sha256: String,
}

#[derive(Serialize)]
struct FragmentBlock {
    key: String,
    family: String,
    file: String,
    size: usize,
    #[serde(rename = "deflatedSize")]
    deflated_size: usize,
    sha256: String,
    ranges: Vec<[u32; 2]>,
    /// The block the page fetches at its start.
    boot: bool,
}

fn parse_size(text: &str) -> Result<usize> {
    let t = text.trim();
    let (number, unit) = match t.find(|c: char| !c.is_ascii_digit() && c != '.') {
        Some(i) => t.split_at(i),
        None => (t, ""),
    };
    let n: f64 = number.parse().with_context(|| format!("not a size: {text}"))?;
    let factor = match unit.to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "kib" | "k" | "kb" => 1024.0,
        "mib" | "m" | "mb" => 1024.0 * 1024.0,
        _ => bail!("not a size: {text} (KiB or MiB)"),
    };
    Ok((n * factor) as usize)
}

fn main() -> Result<()> {
    let mut font = None;
    let (mut id, mut license, mut out) = (None, None, None);
    let mut name = String::new();
    let mut typefaces = Vec::new();
    let mut chunk = 512 * 1024;
    let mut hinting = Hinting::Keep;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().with_context(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--font" => font = Some(PathBuf::from(value()?)),
            "--id" => id = Some(value()?),
            "--name" => name = value()?,
            "--typeface" => typefaces.push(value()?),
            "--license" => license = Some(PathBuf::from(value()?)),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--chunk" => chunk = parse_size(&value()?)?,
            "--hinting" => {
                hinting = match value()?.as_str() {
                    "keep" => Hinting::Keep,
                    "remove" => Hinting::Remove,
                    other => bail!("--hinting {other}: keep or remove"),
                }
            }
            other => bail!("unknown option {other}"),
        }
    }
    let (Some(font), Some(id), Some(license), Some(out)) = (font, id, license, out) else {
        bail!("usage: ibis-font-subsetter --font <file> --id <id> --typeface <name>... --license <file> --out <directory> [--chunk 512KiB] [--hinting keep|remove]");
    };
    if !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        bail!("--id: letters and digits only");
    }
    let bytes = std::fs::read(&font).with_context(|| format!("cannot read {}", font.display()))?;
    let cut = cut(&bytes, &Options { id: id.clone(), name: name.clone(), chunk, hinting })?;
    if !cut.left.is_empty() {
        bail!("{} characters are in no block", cut.left.len());
    }
    let dir = out.join(&id);
    std::fs::create_dir_all(&dir)?;
    for block in &cut.blocks {
        std::fs::write(dir.join(&block.file), &block.bytes)?;
    }
    std::fs::copy(&license, dir.join("OFL.txt")).with_context(|| format!("cannot read {}", license.display()))?;
    let source_hash: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    let font_name = font.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let fragment = Fragment {
        id: id.clone(),
        typeface: typefaces,
        license: "SIL OFL 1.1",
        license_file: format!("{id}/OFL.txt"),
        source: Source { file: font_name.clone(), size: bytes.len(), sha256: source_hash.clone() },
        hinting: if hinting == Hinting::Keep { "keep" } else { "remove" },
        chunk,
        blocks: cut
            .blocks
            .iter()
            .map(|b| FragmentBlock { key: b.key.clone(), family: b.family.clone(), file: format!("{id}/{}", b.file), size: b.size, deflated_size: b.deflated_size, sha256: b.sha256.clone(), ranges: b.ranges.clone(), boot: b.key == "base" })
            .collect(),
    };
    std::fs::write(dir.join("blocks.json"), serde_json::to_string_pretty(&fragment)? + "\n")?;
    let total: usize = cut.blocks.iter().map(|b| b.size).sum();
    let readme = format!(
        "# {id} (blocks for ibisOffice)\n\nThese files are the font `{font_name}` ({} bytes, SHA-256 {source_hash}) cut into blocks by `tools/ibis-font-subsetter` of ibisOffice (the subsetter is skera, MIT OR Apache-2.0). The font is licensed under the SIL Open Font License, Version 1.1 (`OFL.txt`, the licence of the font and its copyright, which are kept in the `name` table of every block too).\n\n## What was changed\n\n- Only the characters of the block were kept, with their outlines{}, metrics, `cmap` (the variation sequences too) and the layout features the default set of the subsetter keeps (among them `vert` and `vrt2`).\n- The family, full, unique and PostScript names are the block's own (`ibis {} <key>`), so that no block carries the name of the font it was cut from. The font's other names (its names in other languages) were left out.\n- {} blocks, {total} bytes in all; a block's deflated size does not pass {chunk} bytes.\n",
        bytes.len(),
        if hinting == Hinting::Keep { " and their hinting" } else { " (the hinting was removed)" },
        if name.trim().is_empty() { id.as_str() } else { name.trim() },
        cut.blocks.len(),
    );
    std::fs::write(dir.join("README.md"), readme)?;
    println!("{id}: {} blocks, {total} bytes ({} deflated)", cut.blocks.len(), cut.blocks.iter().map(|b| b.deflated_size).sum::<usize>());
    for b in &cut.blocks {
        println!("  {} {} bytes, {} deflated, {} ranges", b.file, b.size, b.deflated_size, b.ranges.len());
    }
    Ok(())
}
