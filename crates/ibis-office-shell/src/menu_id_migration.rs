// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The menu ids that were renamed in K0038 (the screens' ids merged into shared ones, abbreviations spelled out, the verb first), for what a
//! person's settings kept under the old name: the クイック アクセス ツール バー's commands and the toolbar orders. They are read as the new
//! id (`current`), so a saved order keeps working. (GENERATED from `tasks/doc/design/k0038-rename-map.tsv` by `scripts/k0038-apply-menu-ids.py`'s
//! lists; do not edit by hand.)

/// Old id, new id.
const RENAMED: &[(&str, &str)] = &[
    ("docx.accessibility-check", "review.accessibility-check"),
    ("docx.compatibility-check", "review.compatibility-check"),
    ("docx.go-to", "edit.go-to"),
    ("docx.image.file", "docx.picture.file"),
    ("docx.merge", "docx.mail-merge"),
    ("docx.page.a3", "page.paper-a3"),
    ("docx.page.a4", "page.paper-a4"),
    ("docx.page.b5", "page.paper-b5"),
    ("docx.para.suppress-auto-hyphens", "docx.paragraph.suppress-auto-hyphens"),
    ("docx.picture.zorder.back", "arrange.send-to-back"),
    ("docx.picture.zorder.backward", "arrange.send-backward"),
    ("docx.picture.zorder.forward", "arrange.bring-forward"),
    ("docx.picture.zorder.front", "arrange.bring-to-front"),
    ("docx.signature-line", "insert.signature-line"),
    ("docx.table.cell-delete", "docx.table.delete-cell"),
    ("docx.table.look.column-bands", "table.banded-columns"),
    ("docx.table.look.first-column", "table.first-column"),
    ("docx.table.look.first-row", "table.header-row"),
    ("docx.table.look.last-column", "table.last-column"),
    ("docx.table.look.last-row", "table.total-row"),
    ("docx.table.look.row-bands", "table.banded-rows"),
    ("docx.toa", "docx.table-of-authorities"),
    ("docx.toa.update", "docx.table-of-authorities.update"),
    ("docx.toc", "docx.table-of-contents"),
    ("docx.toc.mark-level-1", "docx.table-of-contents.mark-level-1"),
    ("docx.toc.mark-level-2", "docx.table-of-contents.mark-level-2"),
    ("docx.toc.mark-level-3", "docx.table-of-contents.mark-level-3"),
    ("docx.toc.update-menu", "docx.table-of-contents.update-menu"),
    ("docx.view-side-by-side", "view.side-by-side"),
    ("docx.view.zoom-100", "view.zoom-100"),
    ("docx.xref", "docx.cross-reference"),
    ("pptx.align.bottom", "arrange.align-bottom"),
    ("pptx.align.center", "arrange.align-center"),
    ("pptx.align.left", "arrange.align-left"),
    ("pptx.align.middle", "arrange.align-middle"),
    ("pptx.align.right", "arrange.align-right"),
    ("pptx.align.top", "arrange.align-top"),
    ("pptx.anim.add", "pptx.animation.add"),
    ("pptx.anim.copy", "pptx.animation.copy"),
    ("pptx.anim.delay", "pptx.animation.delay"),
    ("pptx.anim.duration", "pptx.animation.duration"),
    ("pptx.anim.emphasis", "pptx.animation.emphasis"),
    ("pptx.anim.entrance", "pptx.animation.entrance"),
    ("pptx.anim.exit", "pptx.animation.exit"),
    ("pptx.anim.options", "pptx.animation.options"),
    ("pptx.anim.order.earlier", "pptx.animation.order.earlier"),
    ("pptx.anim.order.later", "pptx.animation.order.later"),
    ("pptx.anim.paste", "pptx.animation.paste"),
    ("pptx.anim.path", "pptx.animation.path"),
    ("pptx.anim.preview", "pptx.animation.preview"),
    ("pptx.anim.start", "pptx.animation.start"),
    ("pptx.anim.trigger", "pptx.animation.trigger"),
    ("pptx.check-accessibility", "review.accessibility-check"),
    ("pptx.compatibility-check", "review.compatibility-check"),
    ("pptx.order.back", "arrange.send-to-back"),
    ("pptx.order.backward", "arrange.send-backward"),
    ("pptx.order.forward", "arrange.bring-forward"),
    ("pptx.order.front", "arrange.bring-to-front"),
    ("pptx.table.band-col", "table.banded-columns"),
    ("pptx.table.band-row", "table.banded-rows"),
    ("pptx.table.first-col", "table.first-column"),
    ("pptx.table.first-row", "table.header-row"),
    ("pptx.table.last-col", "table.last-column"),
    ("pptx.table.last-row", "table.total-row"),
    ("window.prev", "window.previous"),
    ("xlsx.accessibility-check", "review.accessibility-check"),
    ("xlsx.align.bottom", "arrange.align-bottom"),
    ("xlsx.align.center", "arrange.align-center"),
    ("xlsx.align.left", "arrange.align-left"),
    ("xlsx.align.middle", "arrange.align-middle"),
    ("xlsx.align.right", "arrange.align-right"),
    ("xlsx.align.top", "arrange.align-top"),
    ("xlsx.calc-auto", "xlsx.calculate-auto"),
    ("xlsx.calc-auto-no-table", "xlsx.calculate-auto-no-table"),
    ("xlsx.calc-manual", "xlsx.calculate-manual"),
    ("xlsx.comment-delete", "xlsx.delete-comment"),
    ("xlsx.compatibility-check", "review.compatibility-check"),
    ("xlsx.goto", "edit.go-to"),
    ("xlsx.note-new", "xlsx.new-note"),
    ("xlsx.order-back", "arrange.send-to-back"),
    ("xlsx.order-backward", "arrange.send-backward"),
    ("xlsx.order-forward", "arrange.bring-forward"),
    ("xlsx.order-front", "arrange.bring-to-front"),
    ("xlsx.paper-a3", "page.paper-a3"),
    ("xlsx.paper-a4", "page.paper-a4"),
    ("xlsx.paper-b5", "page.paper-b5"),
    ("xlsx.paper-letter", "page.paper-letter"),
    ("xlsx.phonetic-toggle", "xlsx.toggle-phonetic"),
    ("xlsx.pivot-chart-insert", "xlsx.insert-pivot-chart"),
    ("xlsx.print-area-clear", "xlsx.clear-print-area"),
    ("xlsx.print-area-set", "xlsx.set-print-area"),
    ("xlsx.scale-100", "xlsx.print-scale-100"),
    ("xlsx.scale-80", "xlsx.print-scale-80"),
    ("xlsx.signature-line", "insert.signature-line"),
    ("xlsx.slicer-insert", "xlsx.insert-slicer"),
    ("xlsx.table-banded-columns", "table.banded-columns"),
    ("xlsx.table-banded-rows", "table.banded-rows"),
    ("xlsx.table-first-column", "table.first-column"),
    ("xlsx.table-header-row", "table.header-row"),
    ("xlsx.table-last-column", "table.last-column"),
    ("xlsx.table-total-row", "table.total-row"),
    ("xlsx.timeline-insert", "xlsx.insert-timeline"),
    ("xlsx.view-side-by-side", "view.side-by-side"),
];

/// Old prefix, new prefix: every id of the family (`pptx.anim.add` is `pptx.animation.add`).
const FAMILIES: &[(&str, &str)] = &[
    ("pptx.anim.", "pptx.animation."),
    ("add.pptx.anim.", "add.pptx.animation."),
    ("docx.xref", "docx.cross-reference"),
    ("docx.toc", "docx.table-of-contents"),
    ("docx.toa", "docx.table-of-authorities"),
    ("xlsx.calc-", "xlsx.calculate-"),
    ("docx.para.", "docx.paragraph."),
];

/// The id a menu command has now: `id` itself unless it is an old name.
pub fn current(id: &str) -> std::borrow::Cow<'_, str> {
    if let Some((_, new)) = RENAMED.iter().find(|(old, _)| *old == id) {
        return (*new).into();
    }
    for (old, new) in FAMILIES {
        if id == *old || (id.starts_with(old) && (old.ends_with(['.', '-']) || id[old.len()..].starts_with(['.', '-']))) {
            return format!("{new}{}", &id[old.len()..]).into();
        }
    }
    id.into()
}

#[cfg(test)]
mod tests {
    use super::current;

    #[test]
    fn an_old_id_is_read_as_the_new_one() {
        assert_eq!(current("xlsx.order-front"), "arrange.bring-to-front");
        assert_eq!(current("docx.picture.zorder.front"), "arrange.bring-to-front");
        assert_eq!(current("pptx.anim.options"), "pptx.animation.options");
        assert_eq!(current("docx.xref.dialog"), "docx.cross-reference.dialog");
        assert_eq!(current("docx.view.zoom-100"), "view.zoom-100");
        // A current id, and one that merely starts like a family, are as they are.
        assert_eq!(current("file.save"), "file.save");
        assert_eq!(current("docx.tocfoo"), "docx.tocfoo");
        assert_eq!(current("xlsx.calculate-now"), "xlsx.calculate-now");
    }
}
