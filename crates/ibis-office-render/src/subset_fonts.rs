// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The blocks of fonts the browser version takes by name (GitHub issue #157, stage 2): which fonts the page's `loader-manifest.json` has blocks of, the
//! blocks (each a font of its own, `ibis <id> <key>`, with the code points it has) and which of them came. A document's typeface that is one of such a
//! font's names is drawn with the font's base block, then the other blocks that came, in a chain: the text system takes each character from the first of them
//! that has it, so a character is drawn from the block that has it whatever the order the blocks came in.

use std::sync::RwLock;

/// One block of a font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubsetBlock {
    pub key: String,
    /// The family name of the block's font (`ibis <id> <key>`).
    pub family: String,
    /// The code points the block has: ranges (first, last).
    pub ranges: Vec<(u32, u32)>,
    /// Taken when the page starts.
    pub boot: bool,
}

/// A font the page has blocks of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubsetFont {
    pub id: String,
    /// The typefaces a document names for it (compared ignoring case and the spaces at the ends).
    pub typefaces: Vec<String>,
    pub blocks: Vec<SubsetBlock>,
}

#[derive(Default)]
struct State {
    fonts: Vec<SubsetFont>,
    /// (font id, block key) of the blocks that were given to the text system.
    loaded: Vec<(String, String)>,
    /// (font id, chunk key) of the chunks that could not be had: not asked for again by what follows the typing.
    failed: Vec<(String, String)>,
}

static STATE: RwLock<State> = RwLock::new(State { fonts: Vec::new(), loaded: Vec::new(), failed: Vec::new() });

fn state<R>(f: impl FnOnce(&State) -> R) -> R {
    f(&STATE.read().unwrap_or_else(|e| e.into_inner()))
}

fn state_mut<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(&mut STATE.write().unwrap_or_else(|e| e.into_inner()))
}

/// The page's fonts (from the manifest): what was loaded before is forgotten.
pub fn set_fonts(fonts: Vec<SubsetFont>) {
    state_mut(|s| {
        s.fonts = fonts;
        s.loaded.clear();
        s.failed.clear();
    });
}

/// The fonts the page has blocks of.
pub fn fonts() -> Vec<SubsetFont> {
    state(|s| s.fonts.clone())
}

/// A block was given to the text system (`add_fonts`): from now on it is in the chain.
pub fn mark_loaded(font_id: &str, key: &str) {
    state_mut(|s| {
        let entry = (font_id.to_string(), key.to_string());
        if !s.loaded.contains(&entry) {
            s.loaded.push(entry);
        }
    });
}

/// A chunk could not be had (the page's server did not give it): what follows the typing does not ask for it again.
pub fn mark_failed(font_id: &str, key: &str) {
    state_mut(|s| {
        let entry = (font_id.to_string(), key.to_string());
        if !s.failed.contains(&entry) {
            s.failed.push(entry);
        }
    });
}

pub fn is_loaded(font_id: &str, key: &str) -> bool {
    state(|s| s.loaded.contains(&(font_id.to_string(), key.to_string())))
}

fn same_typeface(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// The id of the font that `typeface` is a name of (if the page has blocks of it).
pub fn font_for_typeface(typeface: &str) -> Option<String> {
    state(|s| s.fonts.iter().find(|f| f.typefaces.iter().any(|t| same_typeface(t, typeface))).map(|f| f.id.clone()))
}

/// The family to draw a document's `typeface` with: the base block's, once it is loaded (the first block of the font is its base block).
pub fn base_family_for_typeface(typeface: &str) -> Option<String> {
    state(|s| {
        let font = s.fonts.iter().find(|f| f.typefaces.iter().any(|t| same_typeface(t, typeface)))?;
        let base = font.blocks.first()?;
        s.loaded.contains(&(font.id.clone(), base.key.clone())).then(|| base.family.clone())
    })
}

/// Whether `family` is the family of a block of one of the fonts (loaded or not).
pub fn is_block_family(family: &str) -> bool {
    state(|s| s.fonts.iter().any(|f| f.blocks.iter().any(|b| b.family == family)))
}

/// The families of the loaded blocks that follow the base block of `base_family`'s font, in the order of the blocks: the chain the text system takes the
/// characters of the base block lacks from. Empty when `base_family` is not a base block's family.
pub fn chain_after(base_family: &str) -> Vec<String> {
    state(|s| {
        let Some(font) = s.fonts.iter().find(|f| f.blocks.first().is_some_and(|b| b.family == base_family)) else { return Vec::new() };
        font.blocks.iter().skip(1).filter(|b| s.loaded.contains(&(font.id.clone(), b.key.clone()))).map(|b| b.family.clone()).collect()
    })
}

/// Whether some font the page has chunks of has a chunk that is not taken yet (nothing to follow when all are).
pub fn any_chunk_missing() -> bool {
    state(|s| s.fonts.iter().any(|f| f.blocks.iter().any(|b| !s.loaded.contains(&(f.id.clone(), b.key.clone())) && !s.failed.contains(&(f.id.clone(), b.key.clone())))))
}

/// The keys of the blocks of `font_id` that have a character of `text` and are not loaded yet.
pub fn blocks_needed(font_id: &str, text: &str) -> Vec<String> {
    state(|s| {
        let Some(font) = s.fonts.iter().find(|f| f.id == font_id) else { return Vec::new() };
        let mut keys: Vec<String> = Vec::new();
        for c in text.chars() {
            let c = c as u32;
            if let Some(block) = font.blocks.iter().find(|b| b.ranges.iter().any(|&(a, z)| a <= c && c <= z))
                && !s.loaded.contains(&(font.id.clone(), block.key.clone()))
                && !s.failed.contains(&(font.id.clone(), block.key.clone()))
                && !keys.contains(&block.key)
            {
                keys.push(block.key.clone());
            }
        }
        keys
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(key: &str, ranges: &[(u32, u32)]) -> SubsetBlock {
        SubsetBlock { key: key.into(), family: format!("ibis F {key}"), ranges: ranges.to_vec(), boot: key == "base" }
    }

    #[test]
    fn a_typeface_is_drawn_with_the_base_block_and_the_blocks_that_came_in_a_chain() {
        // One test for the shared state (the tests of a process run side by side).
        set_fonts(vec![SubsetFont { id: "F".into(), typefaces: vec!["Font One".into(), "フォント".into()], blocks: vec![block("base", &[(0x20, 0x7e)]), block("0001", &[(0x4e00, 0x51ff)]), block("0002", &[(0x5200, 0x55ff)])] }]);
        assert_eq!(font_for_typeface(" font one "), Some("F".to_string()));
        assert_eq!(font_for_typeface("Other"), None);
        // Before the base block came the typeface is not drawn with the font.
        assert_eq!(base_family_for_typeface("Font One"), None);
        mark_loaded("F", "base");
        assert_eq!(base_family_for_typeface("Font One"), Some("ibis F base".to_string()));
        assert!(chain_after("ibis F base").is_empty());
        // The chain is in the order of the blocks, not of their coming.
        mark_loaded("F", "0002");
        mark_loaded("F", "0001");
        assert_eq!(chain_after("ibis F base"), ["ibis F 0001", "ibis F 0002"]);
        assert!(chain_after("ibis F 0001").is_empty(), "only a base block starts a chain");
        assert!(is_block_family("ibis F 0002") && !is_block_family("BIZ"));
        // What a text needs: the blocks that have its characters and did not come.
        set_fonts(vec![fonts().remove(0)]);
        mark_loaded("F", "base");
        assert_eq!(blocks_needed("F", "A亜\u{5300}"), ["0001", "0002"]);
        mark_loaded("F", "0001");
        assert_eq!(blocks_needed("F", "A亜\u{5300}"), ["0002"]);
        assert!(blocks_needed("F", "Abc").is_empty());
        assert!(blocks_needed("G", "亜").is_empty());
        // What follows the typing: a chunk of the font that is not taken is still wanted; one that could not be had is not asked for again; all taken, nothing is.
        assert!(any_chunk_missing());
        mark_failed("F", "0002");
        assert!(blocks_needed("F", "A亜\u{5300}").is_empty(), "0001 came, 0002 failed");
        assert!(!any_chunk_missing(), "all taken or failed");
        set_fonts(Vec::new());
        assert!(!any_chunk_missing(), "no font, nothing to follow");
    }
}
