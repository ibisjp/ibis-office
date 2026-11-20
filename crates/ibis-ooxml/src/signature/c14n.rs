// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! XML canonicalization (Canonical XML 1.0 and Exclusive XML Canonicalization 1.0, without comments), for the digests and the
//! signature value of an XML signature. Written from the W3C recommendations (REC-xml-c14n-20010315, xml-exc-c14n).
//!
//! Not done: processing instructions are left out of the output, and an `xml:` attribute inherited from an ancestor of the apex
//! element is not added to it (neither appears in the files Office writes).

use std::collections::BTreeMap;

use crate::xml::{Document, NodeId, NodeKind, XML_NAMESPACE};

/// Prefix ("" for the default namespace) to namespace URI.
type Scope = BTreeMap<String, String>;

/// Exclusive canonicalization (`xml-exc-c14n`) rather than the inclusive one.
pub(crate) fn canonicalize_subtree(doc: &Document, apex: NodeId, exclusive: bool) -> Vec<u8> {
    let mut chain = Vec::new();
    let mut at = doc.parent(apex);
    while let Some(p) = at {
        chain.push(p);
        at = doc.parent(p);
    }
    let mut scope = Scope::new();
    for p in chain.into_iter().rev() {
        declare(doc, p, &mut scope);
    }
    let mut out = String::new();
    element(doc, apex, &scope, &Scope::new(), exclusive, &mut out);
    out.into_bytes()
}

pub(crate) fn canonicalize_document(doc: &Document, exclusive: bool) -> Vec<u8> {
    let mut out = String::new();
    if let Some(root) = doc.root_element() {
        element(doc, root, &Scope::new(), &Scope::new(), exclusive, &mut out);
    }
    out.into_bytes()
}

fn declare(doc: &Document, id: NodeId, scope: &mut Scope) {
    for a in doc.attributes(id) {
        if a.name == "xmlns" {
            scope.insert(String::new(), a.value.clone());
        } else if let Some(p) = a.name.strip_prefix("xmlns:") {
            scope.insert(p.to_string(), a.value.clone());
        }
    }
}

fn element(doc: &Document, id: NodeId, parent_scope: &Scope, rendered: &Scope, exclusive: bool, out: &mut String) {
    let own;
    let scope: &Scope = if doc.attributes(id).iter().any(|a| a.name == "xmlns" || a.name.starts_with("xmlns:")) {
        let mut s = parent_scope.clone();
        declare(doc, id, &mut s);
        own = s;
        &own
    } else {
        parent_scope
    };
    let name = doc.name(id);
    let mut attributes: Vec<(&str, &str, &str, &str)> = Vec::new(); // (namespace, local, name, value)
    for a in doc.attributes(id) {
        if a.name == "xmlns" || a.name.starts_with("xmlns:") {
            continue;
        }
        let (prefix, local) = match a.name.split_once(':') {
            Some((p, l)) => (Some(p), l),
            None => (None, a.name.as_str()),
        };
        let namespace = match prefix {
            None => "",
            Some("xml") => XML_NAMESPACE,
            Some(p) => scope.get(p).map_or("", |s| s.as_str()),
        };
        attributes.push((namespace, local, a.name.as_str(), a.value.as_str()));
    }
    attributes.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));

    // The namespace declarations this element writes.
    let candidates: Vec<(&String, &String)> = if exclusive {
        let mut used: Vec<&str> = vec![name.split_once(':').map_or("", |(p, _)| p)];
        for a in doc.attributes(id) {
            if let Some((p, _)) = a.name.split_once(':') && p != "xmlns" && p != "xml" {
                used.push(p);
            }
        }
        used.sort_unstable();
        used.dedup();
        used.into_iter()
            .filter_map(|p| match scope.get_key_value(p) {
                Some(kv) => Some(kv),
                None => None,
            })
            .collect()
    } else {
        scope.iter().filter(|(p, _)| p.as_str() != "xml").collect()
    };
    let mut written: Vec<(&String, &String)> = Vec::new();
    for (p, u) in candidates {
        let write = match rendered.get(p) {
            Some(r) => r != u,
            None => !u.is_empty(),
        };
        if write {
            written.push((p, u));
        }
    }
    // Exclusive: an unprefixed element with no default namespace in scope, under an output ancestor that has one, writes xmlns="".
    let mut undeclare_default = false;
    if exclusive && !name.contains(':') && !scope.contains_key("") && rendered.get("").is_some_and(|u| !u.is_empty()) {
        undeclare_default = true;
    }
    let next;
    let next_rendered: &Scope = if written.is_empty() && !undeclare_default {
        rendered
    } else {
        let mut r = rendered.clone();
        for (p, u) in &written {
            r.insert((*p).clone(), (*u).clone());
        }
        if undeclare_default {
            r.insert(String::new(), String::new());
        }
        next = r;
        &next
    };

    out.push('<');
    out.push_str(name);
    if undeclare_default {
        out.push_str(" xmlns=\"\"");
    }
    for (p, u) in &written {
        if p.is_empty() {
            out.push_str(" xmlns=\"");
        } else {
            out.push_str(" xmlns:");
            out.push_str(p);
            out.push_str("=\"");
        }
        escape_attribute(u, out);
        out.push('"');
    }
    for (_, _, n, v) in &attributes {
        out.push(' ');
        out.push_str(n);
        out.push_str("=\"");
        escape_attribute(v, out);
        out.push('"');
    }
    out.push('>');
    for &child in doc.children(id) {
        match doc.kind(child) {
            NodeKind::Element => element(doc, child, scope, next_rendered, exclusive, out),
            NodeKind::Text | NodeKind::CData => escape_text(&doc.text(child), out),
            _ => {}
        }
    }
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

fn escape_text(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#xD;"),
            c => out.push(c),
        }
    }
}

fn escape_attribute(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            '\t' => out.push_str("&#x9;"),
            '\n' => out.push_str("&#xA;"),
            '\r' => out.push_str("&#xD;"),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c14n(xml: &str, exclusive: bool) -> String {
        let doc = Document::parse(xml.as_bytes()).unwrap();
        String::from_utf8(canonicalize_document(&doc, exclusive)).unwrap()
    }

    // Example 3.1 of the Canonical XML 1.0 recommendation, the parts this implementation does (no comments, no processing instructions).
    #[test]
    fn spec_example_3_1_without_comments() {
        assert_eq!(c14n("<doc>Hello, world!<!-- Comment 1 --></doc>", false), "<doc>Hello, world!</doc>");
    }

    #[test]
    fn spec_example_3_3_attributes_and_empty_elements() {
        let out = c14n("<doc>\n   <e1   />\n   <e2   ></e2>\n   <e3   name = \"elem3\"   id=\"elem3\"   />\n   <e4   name=\"elem4\"   id=\"elem4\"   ></e4>\n</doc>", false);
        assert_eq!(out, "<doc>\n   <e1></e1>\n   <e2></e2>\n   <e3 id=\"elem3\" name=\"elem3\"></e3>\n   <e4 id=\"elem4\" name=\"elem4\"></e4>\n</doc>");
    }

    #[test]
    fn spec_example_3_4_character_references() {
        let out = c14n("<doc>\n   <text>First line&#x0d;&#10;Second line</text>\n   <value>&#x32;</value>\n   <compute><![CDATA[value>\"0\" && value<\"10\" ?\"valid\":\"error\"]]></compute>\n</doc>", false);
        assert_eq!(out, "<doc>\n   <text>First line&#xD;\nSecond line</text>\n   <value>2</value>\n   <compute>value&gt;\"0\" &amp;&amp; value&lt;\"10\" ?\"valid\":\"error\"</compute>\n</doc>");
    }

    #[test]
    fn namespaces_inclusive_and_exclusive() {
        let xml = "<a xmlns=\"urn:a\" xmlns:x=\"urn:x\" xmlns:y=\"urn:y\"><b x:k=\"1\" xmlns:x=\"urn:x\"><c/></b></a>";
        assert_eq!(c14n(xml, false), "<a xmlns=\"urn:a\" xmlns:x=\"urn:x\" xmlns:y=\"urn:y\"><b x:k=\"1\"><c></c></b></a>");
        assert_eq!(c14n(xml, true), "<a xmlns=\"urn:a\"><b xmlns:x=\"urn:x\" x:k=\"1\"><c></c></b></a>");
    }

    #[test]
    fn subtree_takes_the_namespaces_in_scope() {
        let doc = Document::parse(b"<r xmlns=\"urn:r\" xmlns:p=\"urn:p\"><s Id=\"1\"><t/></s></r>").unwrap();
        let s = doc.path(doc.root_element().unwrap(), &["s"]).unwrap();
        assert_eq!(String::from_utf8(canonicalize_subtree(&doc, s, false)).unwrap(), "<s xmlns=\"urn:r\" xmlns:p=\"urn:p\" Id=\"1\"><t></t></s>");
        assert_eq!(String::from_utf8(canonicalize_subtree(&doc, s, true)).unwrap(), "<s xmlns=\"urn:r\" Id=\"1\"><t></t></s>");
    }
}
