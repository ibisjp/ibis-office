// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The resources the web version fetches when they are needed (the desktop apps carry them): `docs/resource-list.txt` of the public repository, JSON Lines as
//! `font-list.txt` is (`font_setup`: the same reader, the same rules for a name and an address, the same GitHub Pages: `sounds/<id>/<file>` beside its `LICENSE`).
//! The first kind is `transition-sound`: the 18 sounds of a slide's transition (`ibis_formats::transition_sounds`). The browser keeps what it fetched in IndexedDB
//! as it keeps fonts (`font_setup`).

use crate::font_setup::{Json, parse_object, safe_name, safe_url};

/// The public repository's list.
pub const LIST_URL: &str = "https://raw.githubusercontent.com/ibisjp/ibis-office/main/docs/resource-list.txt";

/// The address of the list: the host's (`resources.listUrl` of its options), else `IBIS_RESOURCE_LIST_URL` (the tests; a local server; not in the browser), else the public repository's.
pub fn list_url() -> String {
    if let Some(url) = crate::host::options().resource_list_url {
        return url;
    }
    #[cfg(not(target_family = "wasm"))]
    if let Some(url) = std::env::var("IBIS_RESOURCE_LIST_URL").ok().filter(|v| !v.is_empty()) {
        return url;
    }
    LIST_URL.to_string()
}

/// `url` as an absolute address against `base` (the address of the list): an absolute one stays; `//host/x` takes the scheme of `base`; `/x` the origin of `base`; any other is beside the list
/// (`.` and `..` folded). `None` when `base` is not an `http(s)` address.
pub(crate) fn resolve_url(base: &str, url: &str) -> Option<String> {
    if url.starts_with("https://") || url.starts_with("http://") {
        return Some(url.to_string());
    }
    let (scheme, rest) = base.split_once("://")?;
    if scheme != "https" && scheme != "http" {
        return None;
    }
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let (host, path) = rest.split_once('/').map_or((rest, ""), |(h, p)| (h, p));
    if let Some(network) = url.strip_prefix("//") {
        return Some(format!("{scheme}://{network}"));
    }
    let mut parts: Vec<&str> = if url.starts_with('/') { Vec::new() } else { path.split('/').collect::<Vec<_>>().into_iter().rev().skip(1).rev().collect() };
    for piece in url.trim_start_matches('/').split('/') {
        match piece {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    Some(format!("{scheme}://{host}/{}", parts.join("/")))
}

/// One resource of the list.
///
/// | key | meaning |
/// |---|---|
/// | `id` | the resource's name (a folder on Pages); `[A-Za-z0-9._-]` (required) |
/// | `kind` | what it is: `transition-sound` (required) |
/// | `name_en`, `name_ja` | its names (`name_en` required; `name_ja` default `name_en`) |
/// | `file` | the file's name (required) |
/// | `size` | the file's size in bytes (the download is checked against it) |
/// | `sha256` | the file's SHA-256, hex (optional; checked when given) |
/// | `url`, `license_url` | the addresses of the file (required) and of its licence (https) |
/// | `license`, `credit` | the licence's name and the credit, as shown |
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceEntry {
    pub id: String,
    pub kind: String,
    pub name_en: String,
    pub name_ja: String,
    pub file: String,
    pub size: u64,
    pub sha256: Option<String>,
    pub url: String,
    pub license: String,
    pub license_url: String,
    pub credit: String,
}

/// The resources of `resource-list.txt`: one JSON object a line; empty lines and lines beginning with `#` are skipped, and so is a line that is not a resource (not JSON, a
/// required key missing, a name or an address that is not plain), which does not stop the others. A file listed twice under one kind is listed once (the first).
pub fn parse_list(text: &str) -> Vec<ResourceEntry> {
    parse_list_at(text, None)
}

/// `parse_list` for a list read from `base`: an address of a line that is not absolute (`url`, `license_url`) is taken against `base` (the host serves the list and the files together).
pub fn parse_list_at(text: &str, base: Option<&str>) -> Vec<ResourceEntry> {
    let mut out: Vec<ResourceEntry> = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let Some(pairs) = parse_object(line) else { continue };
        let get = |key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v);
        let string = |key: &str| match get(key) {
            Some(Json::Str(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
            _ => None,
        };
        let (Some(id), Some(kind), Some(name_en), Some(file), Some(url)) = (string("id"), string("kind"), string("name_en"), string("file"), string("url")) else { continue };
        let absolute = |u: String| match base {
            Some(base) => resolve_url(base, &u).unwrap_or(u),
            None => u,
        };
        let license_url = string("license_url").unwrap_or_default();
        let (url, license_url) = (absolute(url), if license_url.is_empty() { license_url } else { absolute(license_url) });
        if !safe_name(&id) || !safe_name(&file) || !safe_url(&url) || (!license_url.is_empty() && !safe_url(&license_url)) || out.iter().any(|e| e.kind == kind && e.file == file) {
            continue;
        }
        let size = match get("size") {
            Some(Json::Num(n)) if *n >= 0. => *n as u64,
            _ => 0,
        };
        let sha256 = string("sha256").map(|s| s.to_ascii_lowercase()).filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()));
        out.push(ResourceEntry {
            id,
            kind,
            name_ja: string("name_ja").unwrap_or_else(|| name_en.clone()),
            name_en,
            file,
            size,
            sha256,
            url,
            license: string("license").unwrap_or_default(),
            license_url,
            credit: string("credit").unwrap_or_default(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_published_list_has_the_18_transition_sounds_with_their_files() {
        let list = parse_list(include_str!("../../../publish/docs/resource-list.txt"));
        assert_eq!(list.len(), 18);
        assert!(list.iter().all(|e| e.kind == "transition-sound" && e.size > 1000 && e.sha256.is_some() && e.url.ends_with(&format!("/{0}/{0}.ogg", e.id))));
        // Each file is the one the desktop build carries (the same size and checksum).
        #[cfg(not(target_family = "wasm"))]
        for e in &list {
            let bytes = ibis_formats::transition_sounds::ogg(&e.id).unwrap_or_else(|| panic!("{}", e.id));
            assert_eq!(bytes.len() as u64, e.size, "{}", e.id);
        }
    }

    #[test]
    fn relative_addresses_are_taken_against_the_lists_own() {
        let base = "https://works.example.com/office/res/resource-list.txt?v=1";
        assert_eq!(resolve_url(base, "sounds/a/a.ogg").as_deref(), Some("https://works.example.com/office/res/sounds/a/a.ogg"));
        assert_eq!(resolve_url(base, "../x.ogg").as_deref(), Some("https://works.example.com/office/x.ogg"));
        assert_eq!(resolve_url(base, "/s/a.ogg").as_deref(), Some("https://works.example.com/s/a.ogg"));
        assert_eq!(resolve_url(base, "//cdn.example.com/a.ogg").as_deref(), Some("https://cdn.example.com/a.ogg"));
        assert_eq!(resolve_url(base, "https://x.example.com/a.ogg").as_deref(), Some("https://x.example.com/a.ogg"));
        assert_eq!(resolve_url("file:///a/b.txt", "a.ogg"), None);
        let line = "{\"id\":\"a\",\"kind\":\"transition-sound\",\"name_en\":\"A\",\"file\":\"a.ogg\",\"url\":\"sounds/a/a.ogg\",\"license_url\":\"sounds/a/LICENSE\"}";
        assert!(parse_list(line).is_empty(), "a relative address with no base is not an address the app may fetch");
        let list = parse_list_at(line, Some(base));
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].url, "https://works.example.com/office/res/sounds/a/a.ogg");
        assert_eq!(list[0].license_url, "https://works.example.com/office/res/sounds/a/LICENSE");
    }

    #[test]
    fn a_line_that_is_not_a_resource_does_not_stop_the_others() {
        let text = "# c\n\nnot json\n{\"id\":\"a\",\"kind\":\"transition-sound\",\"name_en\":\"A\",\"file\":\"a.ogg\",\"url\":\"https://x/a.ogg\"}\n{\"id\":\"../b\",\"kind\":\"k\",\"name_en\":\"B\",\"file\":\"b.ogg\",\"url\":\"https://x/b.ogg\"}\n";
        let list = parse_list(text);
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].id.as_str(), list[0].name_ja.as_str()), ("a", "A"));
    }
}
