// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Helpers over `roxmltree` that match elements and attributes by local name.
//!
//! OOXML element names are unique enough per context that the namespace
//! prefix never needs checking, and matching by local name keeps the reader
//! independent of the prefixes a producer chose.

use roxmltree::Node;

pub(crate) fn elements<'a, 'i>(node: Node<'a, 'i>) -> impl Iterator<Item = Node<'a, 'i>> {
    node.children().filter(Node::is_element)
}

pub(crate) fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    elements(node).find(|n| n.tag_name().name() == name)
}

pub(crate) fn children<'a, 'i>(node: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    elements(node).filter(move |n| n.tag_name().name() == name)
}

/// Follow a path of child element names.
pub(crate) fn path<'a, 'i>(node: Node<'a, 'i>, names: &[&str]) -> Option<Node<'a, 'i>> {
    names.iter().try_fold(node, |n, name| child(n, name))
}

/// An attribute by local name, whatever its namespace (`r:embed` is "embed").
pub(crate) fn attr<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes().find(|a| a.name() == name).map(|a| a.value())
}

pub(crate) fn attr_i64(node: Node, name: &str) -> Option<i64> {
    attr(node, name)?.trim().parse().ok()
}

pub(crate) fn attr_bool(node: Node, name: &str) -> Option<bool> {
    match attr(node, name)? {
        "1" | "true" | "on" => Some(true),
        "0" | "false" | "off" => Some(false),
        _ => None,
    }
}

/// The position of `node` from the root element: its index among its
/// parent's element children, at each level. `ibis_ooxml`'s tree finds the
/// same element by it (`XmlDocument::by_path`), which is how the writer
/// finds what a reader read.
pub(crate) fn element_path(node: Node) -> Vec<u32> {
    let mut path = Vec::new();
    let mut at = node;
    while let Some(parent) = at.parent() {
        if !parent.is_element() {
            break;
        }
        path.push(elements(parent).position(|c| c == at).unwrap_or(0) as u32);
        at = parent;
    }
    path.reverse();
    path
}
