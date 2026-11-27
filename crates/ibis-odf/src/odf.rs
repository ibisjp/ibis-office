// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/249)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

use roxmltree::Node;

pub(crate) const TABLE: &str = "urn:oasis:names:tc:opendocument:xmlns:table:1.0";
pub(crate) const TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";

/// Visit elements in source order, replacing selected containers with their
/// children. Other elements are yielded whole, without visiting their children.
pub(crate) fn through<'a, 'i>(
    root: Node<'a, 'i>,
    mut descend: impl FnMut(Node<'a, 'i>) -> bool,
) -> impl Iterator<Item = Node<'a, 'i>> {
    let mut stack = vec![root.children()];
    std::iter::from_fn(move || {
        while let Some(children) = stack.last_mut() {
            let Some(node) = children.find(Node::is_element) else {
                stack.pop();
                continue;
            };
            if descend(node) {
                stack.push(node.children());
            } else {
                return Some(node);
            }
        }
        None
    })
}

pub(crate) fn visible_section(node: Node) -> bool {
    node.has_tag_name((TEXT, "section"))
        && !matches!(
            node.attribute((TEXT, "display")),
            Some("none" | "condition")
        )
}

pub(crate) fn text_container(node: Node) -> bool {
    visible_section(node)
        || (node.tag_name().namespace() == Some(TEXT)
            && matches!(
                node.tag_name().name(),
                "list" | "list-item" | "list-header" | "numbered-paragraph"
            ))
}

pub(crate) fn table_rows<'a, 'i>(table: Node<'a, 'i>) -> impl Iterator<Item = Node<'a, 'i>> {
    through(table, |node| {
        node.tag_name().namespace() == Some(TABLE)
            && matches!(
                node.tag_name().name(),
                "table-row-group" | "table-header-rows" | "table-rows"
            )
    })
    .filter(|node| node.has_tag_name((TABLE, "table-row")))
}

pub(crate) fn table_columns<'a, 'i>(table: Node<'a, 'i>) -> impl Iterator<Item = Node<'a, 'i>> {
    through(table, |node| {
        node.tag_name().namespace() == Some(TABLE)
            && matches!(
                node.tag_name().name(),
                "table-column-group" | "table-header-columns" | "table-columns"
            )
    })
    .filter(|node| node.has_tag_name((TABLE, "table-column")))
}

pub(crate) fn row_hidden(row: Node) -> bool {
    axis_hidden(row, "table-row-group")
}

pub(crate) fn column_hidden(column: Node) -> bool {
    axis_hidden(column, "table-column-group")
}

fn axis_hidden(node: Node, group: &str) -> bool {
    matches!(
        node.attribute((TABLE, "visibility")),
        Some("collapse" | "filter")
    ) || node
        .ancestors()
        .skip(1)
        .take_while(|n| !n.has_tag_name((TABLE, "table")))
        .any(|n| {
            n.has_tag_name((TABLE, group))
                && matches!(n.attribute((TABLE, "display")), Some("false" | "0"))
        })
}
