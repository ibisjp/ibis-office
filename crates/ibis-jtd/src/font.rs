// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! FontV.01 records from the published format notes, checked against local
//! stream boundaries. No character-selector-to-font mapping is assumed.
use ibis_formats::Error;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontRecord {
    pub index: u16,
    pub face: String,
    pub style: String,
    pub weight: u32,
    pub italic: bool,
    pub charset: u8,
}

fn invalid() -> Error {
    Error::new("JTDのフォント表が未対応または不正です")
}

pub fn parse(data: &[u8]) -> Result<Vec<FontRecord>, Error> {
    if data.len() > 1024 * 1024 || data.get(..8) != Some(b"FontV.01") {
        return Err(invalid());
    }
    let count =
        u16::from_be_bytes(data.get(8..10).ok_or_else(invalid)?.try_into().unwrap()) as usize;
    if count > 1024 {
        return Err(invalid());
    }
    let mut at = 10usize;
    let mut seen = BTreeSet::new();
    let mut records = Vec::new();
    for _ in 0..count {
        let index = u16::from_be_bytes(
            data.get(at..at + 2)
                .ok_or_else(invalid)?
                .try_into()
                .unwrap(),
        );
        at += 2;
        if !seen.insert(index) {
            return Err(invalid());
        }
        let fixed = data.get(at..at + 28).ok_or_else(invalid)?;
        at += 28;
        if u32::from_be_bytes(fixed[16..20].try_into().unwrap()) > 1000 || fixed[20] > 1 {
            return Err(invalid());
        }
        let face = string(data, &mut at)?;
        data.get(at..at + 12).ok_or_else(invalid)?;
        at += 12;
        let style = string(data, &mut at)?;
        if face.is_empty() || face.chars().any(char::is_control) {
            return Err(invalid());
        }
        records.push(FontRecord {
            index,
            face,
            style,
            weight: u32::from_be_bytes(fixed[16..20].try_into().unwrap()),
            italic: fixed[20] != 0,
            charset: fixed[23],
        });
    }
    if at != data.len() {
        return Err(invalid());
    }
    Ok(records)
}

fn string(data: &[u8], at: &mut usize) -> Result<String, Error> {
    let mut units = Vec::new();
    for _ in 0..1024 {
        let unit = u16::from_be_bytes(
            data.get(*at..*at + 2)
                .ok_or_else(invalid)?
                .try_into()
                .unwrap(),
        );
        *at += 2;
        if unit == 0 {
            return String::from_utf16(&units).map_err(|_| invalid());
        }
        units.push(unit);
    }
    Err(invalid())
}

#[cfg(test)]
pub(super) fn fixture(fonts: &[(u16, &str, u32, bool)]) -> Vec<u8> {
    let mut data = b"FontV.01".to_vec();
    data.extend_from_slice(&(fonts.len() as u16).to_be_bytes());
    for (index, name, weight, italic) in fonts {
        data.extend_from_slice(&index.to_be_bytes());
        let mut fixed = [0; 28];
        fixed[16..20].copy_from_slice(&weight.to_be_bytes());
        fixed[20] = u8::from(*italic);
        fixed[23] = 128;
        data.extend(fixed);
        for unit in name.encode_utf16().chain([0]) {
            data.extend_from_slice(&unit.to_be_bytes());
        }
        data.extend([0; 12]);
        for unit in "標準".encode_utf16().chain([0]) {
            data.extend_from_slice(&unit.to_be_bytes());
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn font_names_are_utf16_and_indices_need_not_be_contiguous() {
        let records = parse(&fixture(&[
            (0, "ＭＳ ゴシック", 400, false),
            (9, "ＭＳ 明朝", 700, true),
        ]))
        .unwrap();
        assert_eq!(records[1].index, 9);
        assert_eq!(records[1].face, "ＭＳ 明朝");
        assert_eq!(records[1].weight, 700);
        assert!(records[1].italic);
    }
    #[test]
    fn truncated_ambiguous_or_extended_tables_are_not_guessed() {
        let mut bytes = fixture(&[(0, "A", 400, false)]);
        assert!(parse(&bytes[..bytes.len() - 1]).is_err());
        bytes.extend([0, 0]);
        assert!(parse(&bytes).is_err());
        assert!(parse(&fixture(&[(0, "A", 400, false), (0, "B", 400, false)])).is_err());
    }
}
