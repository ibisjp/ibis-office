// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Cuts a font (OFL) into the blocks the browser version fetches (GitHub issue #157, stage 2): a base block (ASCII and the repertoire of JIS X 0208) and
//! blocks of the rest of the font's characters in the order of their code points, each of a size (stored: deflated) that does not pass a limit. Each block is a
//! font of its own, with its own family name (`ibis <id> <key>`), so the text system takes them as a chain (a font and its fallbacks) and each
//! character is drawn from the block that has it.
//!
//! What a block keeps of the font (the same rules as HarfBuzz's subsetter, `skera`): the outlines and their hinting, the metrics, `cmap` (format 14, the
//! variation sequences, too: the selectors are put in the set of every block), the layout features `vert`/`vrt2` and the rest of the default set, the
//! names (the copyright and the licence stay; the family, full and PostScript names are the block's own; no other name of the font is kept). The output is
//! the same for the same input.

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};
use skera::{DEFAULT_LAYOUT_FEATURES, Plan, SubsetFlags, subset_font};
use skrifa::MetadataProvider;
use write_fonts::read::{FontRef, collections::IntSet, types::{GlyphId, NameId, Tag}};

/// The variation selectors (VS1-16 and VS17-256): in the set of every block, so that the font's `cmap` format 14 stays.
const VARIATION_SELECTORS: [(u32, u32); 2] = [(0xFE00, 0xFE0F), (0xE0100, 0xE01EF)];

/// The repertoire of JIS X 0208 as ranges of code points (made from the standard; see the file).
const JISX0208: &str = include_str!("../data/jisx0208.txt");

/// The tables dropped (what skera's own command line drops by default).
const DROP_TABLES: [&[u8; 4]; 16] = [b"morx", b"mort", b"kerx", b"kern", b"JSTF", b"DSIG", b"EBDT", b"EBLC", b"EBSC", b"SVG ", b"PCLT", b"LTSH", b"Feat", b"Glat", b"Gloc", b"Silf"];

/// The name records a block keeps: the copyright (0), the family (1), the style (2), the unique id (3), the full name (4), the version (5), the PostScript name (6),
/// the licence (13) and its address (14). The family, the full name, the unique id and the PostScript name are given the block's own.
const NAME_IDS: [u16; 9] = [0, 1, 2, 3, 4, 5, 6, 13, 14];

/// What is done with the font's hinting (the bytecode and the `gasp` table the renderer's hinting uses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hinting {
    Keep,
    Remove,
}

/// One block of a cut font.
#[derive(Debug, Serialize, Clone)]
pub struct Block {
    /// `base`, or the number of the block (`0001`, ...).
    pub key: String,
    /// The family name of the block's font (`ibis <id> <key>`), the one the text system is asked for.
    pub family: String,
    /// The file name (`<key>.<first 12 digits of the SHA-256>.ttf`).
    pub file: String,
    pub size: usize,
    /// The size deflated (what a server that compresses sends, and what the block's limit is of).
    #[serde(rename = "deflatedSize")]
    pub deflated_size: usize,
    pub sha256: String,
    /// The code points the block has, as ranges (first, last), the ASCII and the selectors left out of the count of what is the block's own.
    pub ranges: Vec<[u32; 2]>,
    #[serde(skip)]
    pub bytes: Vec<u8>,
}

/// The cut font.
#[derive(Debug)]
pub struct CutFont {
    pub blocks: Vec<Block>,
    /// The code points of the font that no block has (none unless the font has what a plan cannot cut).
    pub left: BTreeSet<u32>,
}

/// What the cutting needs to know besides the font.
pub struct Options {
    /// The id of the font (`BIZUDGothic`): in the PostScript names and the file names.
    pub id: String,
    /// The name of the font as it is shown (`BIZ UDGothic`): in the family names (`ibis <name> <key>`). The id when empty.
    pub name: String,
    /// A block's limit after deflating, in bytes (the base block is not cut by it).
    pub chunk: usize,
    pub hinting: Hinting,
}

/// The code points of the font's `cmap` (without the variation selectors, which belong to every block).
pub fn font_code_points(font: &FontRef) -> BTreeSet<u32> {
    let selectors: BTreeSet<u32> = VARIATION_SELECTORS.iter().flat_map(|(a, b)| *a..=*b).collect();
    font.charmap().mappings().map(|(c, _)| c).filter(|c| !selectors.contains(c)).collect()
}

/// The base block's code points: ASCII, the repertoire of JIS X 0208, and the full width forms of the ASCII (the font's own cmap decides which exist).
pub fn base_set() -> BTreeSet<u32> {
    let mut set: BTreeSet<u32> = (0x20..0x7F).collect();
    for line in JISX0208.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let mut parts = line.split_whitespace().map(|p| u32::from_str_radix(p, 16).expect("a hex number in jisx0208.txt"));
        let (first, last) = (parts.next().expect("first"), parts.next().expect("last"));
        set.extend(first..=last);
    }
    set
}

fn sha_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// The size of `bytes` deflated (zlib level 9 stands for what the transfer's gzip or Brotli sends).
fn deflated_size(bytes: &[u8]) -> usize {
    miniz_oxide::deflate::compress_to_vec(bytes, 9).len()
}

/// The ranges of the runs of consecutive code points of `set`.
pub fn ranges_of(set: &BTreeSet<u32>) -> Vec<[u32; 2]> {
    let mut out: Vec<[u32; 2]> = Vec::new();
    for &c in set {
        match out.last_mut() {
            Some(last) if last[1] + 1 == c => last[1] = c,
            _ => out.push([c, c]),
        }
    }
    out
}

/// The font cut to `code_points` (and the selectors), named `family`.
pub fn subset(font_bytes: &[u8], code_points: &BTreeSet<u32>, family: &str, postscript: &str, unique: &str, hinting: Hinting) -> Result<Vec<u8>> {
    let font = FontRef::new(font_bytes).context("the font cannot be read")?;
    // A truncated font (its table directory cut short) opens, but skera panics at the first table it needs: the tables every cut reads are checked first.
    for tag in [b"head", b"hhea", b"hmtx", b"maxp", b"cmap"] {
        if font.table_data(Tag::new(tag)).is_none() {
            bail!("the font has no {} table", String::from_utf8_lossy(tag));
        }
    }
    let mut unicodes: IntSet<u32> = code_points.iter().copied().collect();
    for (a, b) in VARIATION_SELECTORS {
        unicodes.extend_unsorted(a..=b);
    }
    let mut drop: IntSet<Tag> = DROP_TABLES.iter().map(|t| Tag::new(t)).collect();
    let mut flags = SubsetFlags::default();
    if hinting == Hinting::Remove {
        flags |= SubsetFlags::SUBSET_FLAGS_NO_HINTING;
        drop.insert(Tag::new(b"gasp"));
    }
    let layout_scripts: IntSet<Tag> = IntSet::all();
    let features: IntSet<Tag> = DEFAULT_LAYOUT_FEATURES.iter().copied().collect();
    let name_ids: IntSet<NameId> = NAME_IDS.iter().map(|i| NameId::new(*i)).collect();
    // Only the English (Windows) records stay: the font's names in other languages carry its own name.
    let languages: IntSet<u16> = IntSet::from_iter([0x0409u16]);
    let mut plan = Plan::new(&IntSet::<GlyphId>::empty(), &unicodes, &font, flags, &drop, &layout_scripts, &features, &name_ids, &languages);
    for (id, text) in [(1u16, family), (4, family), (6, postscript), (3, unique)] {
        plan.override_name_table(NameId::new(id), 3, 1, 0x0409, Some(text)).map_err(|e| anyhow::anyhow!("name {id}: {e}"))?;
    }
    subset_font(&font, &plan).map_err(|e| anyhow::anyhow!("the font could not be cut: {e}"))
}

fn names_for(id: &str, name: &str, key: &str, source_hash: &str) -> (String, String, String) {
    let compact: String = id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let shown = if name.trim().is_empty() { id } else { name.trim() };
    (format!("ibis {shown} {key}"), format!("ibis-{compact}-{key}"), format!("ibis-{compact}-{key}-{}", &source_hash[..12]))
}

fn make_block(font_bytes: &[u8], source_hash: &str, options: &Options, key: &str, set: &BTreeSet<u32>) -> Result<Block> {
    let (family, postscript, unique) = names_for(&options.id, &options.name, key, source_hash);
    let bytes = subset(font_bytes, set, &family, &postscript, &unique, options.hinting)?;
    let hash = sha_hex(&bytes);
    Ok(Block {
        key: key.to_string(),
        family,
        file: format!("{key}.{}.ttf", &hash[..12]),
        size: bytes.len(),
        deflated_size: deflated_size(&bytes),
        sha256: hash,
        ranges: ranges_of(set),
        bytes,
    })
}

/// Cuts the font: the base block, then the rest in the order of the code points, in blocks whose deflated size does not pass `options.chunk`.
pub fn cut(font_bytes: &[u8], options: &Options) -> Result<CutFont> {
    if options.chunk < 4096 {
        bail!("the block limit is too small");
    }
    let font = FontRef::new(font_bytes).context("the font cannot be read")?;
    let all = font_code_points(&font);
    let source_hash = sha_hex(font_bytes);
    let base: BTreeSet<u32> = base_set().intersection(&all).copied().collect();
    let mut blocks = vec![make_block(font_bytes, &source_hash, options, "base", &base)?];
    let rest: Vec<u32> = all.difference(&base).copied().collect();
    // What a character of the rest costs, deflated: the first guess, then the last block's own.
    let mut per_char = 300.0f64;
    let (mut at, mut number) = (0usize, 1usize);
    while at < rest.len() {
        let mut count = (((options.chunk as f64) / per_char) as usize).clamp(1, rest.len() - at);
        let block = loop {
            let set: BTreeSet<u32> = rest[at..at + count].iter().copied().collect();
            let block = make_block(font_bytes, &source_hash, options, &format!("{number:04}"), &set)?;
            if block.deflated_size <= options.chunk {
                // Close to the limit (or the last one): taken; else more are tried.
                let remaining = rest.len() - at - count;
                if remaining == 0 || block.deflated_size as f64 >= options.chunk as f64 * 0.9 {
                    break block;
                }
                let more = ((count as f64) * (options.chunk as f64 * 0.97 / block.deflated_size as f64)).floor() as usize;
                if more <= count {
                    break block;
                }
                count = more.min(rest.len() - at);
                continue;
            }
            if count == 1 {
                bail!("one character does not fit the limit");
            }
            count = (((count as f64) * (options.chunk as f64 * 0.95 / block.deflated_size as f64)).floor() as usize).clamp(1, count - 1);
        };
        per_char = (block.deflated_size as f64 / count as f64).max(1.0);
        at += count;
        number += 1;
        blocks.push(block);
    }
    let have: BTreeSet<u32> = blocks.iter().flat_map(|b| b.ranges.iter().flat_map(|r| r[0]..=r[1])).collect();
    Ok(CutFont { left: all.difference(&have).copied().collect(), blocks })
}
