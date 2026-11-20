// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Reading slicers (スライサー): the slicers of a sheet
//! (`xl/slicers/slicer*.xml`, MS-XLSX §2.6.x `x14:slicer`) and the caches the
//! workbook lists (`xl/slicerCaches/slicerCache*.xml`,
//! `x14:slicerCacheDefinition`) that say what each one filters. The drawing
//! (`sle:slicer`) places it on the sheet.

use ibis_doc_model::slicer::{Slicer, SlicerSource};
use roxmltree::Node;

use crate::xml::{attr, attr_bool, attr_i64, child, children, elements};

/// What a slicer cache filters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SlicerCache {
    pub name: String,
    /// The field's (column's) name (`sourceName`).
    pub source_name: String,
    /// The pivot tables it filters (`pivotTables/pivotTable/@name`).
    pub pivots: Vec<String>,
    /// The table it filters (`x15:tableSlicerCache/@tableId`).
    pub table_id: Option<u32>,
    /// Whether the items are sorted in descending order (`sortOrder="descending"`).
    pub descending: bool,
}

/// A slicer cache definition's root.
pub(crate) fn read_cache(root: Node) -> Option<SlicerCache> {
    if root.tag_name().name() != "slicerCacheDefinition" {
        return None;
    }
    let mut cache = SlicerCache { name: attr(root, "name")?.to_string(), source_name: attr(root, "sourceName").unwrap_or("").to_string(), ..Default::default() };
    if let Some(p) = child(root, "pivotTables") {
        cache.pivots = children(p, "pivotTable").filter_map(|t| attr(t, "name").map(str::to_string)).collect();
    }
    cache.table_id = root.descendants().find(|n| n.tag_name().name() == "tableSlicerCache").and_then(|n| attr_i64(n, "tableId")).map(|v| v.max(0) as u32);
    cache.descending = root.descendants().any(|n| matches!(n.tag_name().name(), "tabular" | "tableSlicerCache") && attr(n, "sortOrder") == Some("descending"));
    Some(cache)
}

/// The slicers of a `slicers` part, with what each filters (`caches` by
/// name; `table_names` the tables by their `id`).
pub(crate) fn read_slicers(root: Node, caches: &std::collections::HashMap<String, SlicerCache>, table_names: &std::collections::HashMap<u32, String>) -> Vec<Slicer> {
    if root.tag_name().name() != "slicers" {
        return Vec::new();
    }
    elements(root)
        .filter(|n| n.tag_name().name() == "slicer")
        .filter_map(|n| {
            let name = attr(n, "name")?.to_string();
            let cache_name = attr(n, "cache")?.to_string();
            let cache = caches.get(&cache_name)?;
            let source = match cache.table_id {
                Some(id) => SlicerSource::Table { table: table_names.get(&id)?.clone(), field: cache.source_name.clone() },
                None => SlicerSource::Pivot { field: cache.source_name.clone(), pivots: cache.pivots.clone() },
            };
            Some(Slicer {
                caption: attr(n, "caption").unwrap_or(&name).to_string(),
                name,
                cache: cache_name,
                style: attr(n, "style").unwrap_or("").to_string(),
                columns: attr_i64(n, "columnCount").map_or(1, |v| v.clamp(1, 20) as u32),
                row_height: attr_i64(n, "rowHeight").unwrap_or(0),
                show_caption: attr_bool(n, "showCaption").unwrap_or(true),
                descending: cache.descending,
                source,
                items: Vec::new(),
            })
        })
        .collect()
}

/// The slicer a drawing's `graphicFrame` holds (`sle:slicer/@name`), if it is one.
pub(crate) fn drawing_slicer_name<'a>(anchor: Node<'a, '_>) -> Option<&'a str> {
    anchor.descendants().find(|n| n.is_element() && n.tag_name().name() == "slicer" && n.tag_name().namespace().is_some_and(|ns| ns.ends_with("/drawing/2010/slicer"))).and_then(|n| attr(n, "name"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const CACHE_PIVOT: &str = r#"<slicerCacheDefinition xmlns="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main" name="Slicer_地域" sourceName="地域"><pivotTables><pivotTable tabId="2" name="売上"/></pivotTables><data><tabular pivotCacheId="1"><items count="2"><i x="0" s="1"/><i x="1"/></items></tabular></data></slicerCacheDefinition>"#;
    const CACHE_TABLE: &str = r#"<slicerCacheDefinition xmlns="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main" xmlns:x15="http://schemas.microsoft.com/office/spreadsheetml/2010/11/main" name="Slicer_品名" sourceName="品名"><extLst><ext uri="{2F2917AC-EB37-4324-AD4E-5DD8C200BD13}"><x15:tableSlicerCache tableId="3" column="2" sortOrder="descending"/></ext></extLst></slicerCacheDefinition>"#;
    const SLICERS: &str = r#"<slicers xmlns="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"><slicer name="地域" cache="Slicer_地域" caption="地域" columnCount="2" rowHeight="241300" style="SlicerStyleLight2"/><slicer name="品名" cache="Slicer_品名" showCaption="0"/></slicers>"#;

    #[test]
    fn slicers_are_read_with_what_they_filter() {
        let mut caches = HashMap::new();
        for xml in [CACHE_PIVOT, CACHE_TABLE] {
            let doc = roxmltree::Document::parse(xml).unwrap();
            let c = read_cache(doc.root_element()).unwrap();
            caches.insert(c.name.clone(), c);
        }
        let tables: HashMap<u32, String> = [(3, "売上表".to_string())].into();
        let doc = roxmltree::Document::parse(SLICERS).unwrap();
        let slicers = read_slicers(doc.root_element(), &caches, &tables);
        assert_eq!(slicers.len(), 2);
        assert_eq!(slicers[0].source, SlicerSource::Pivot { field: "地域".into(), pivots: vec!["売上".into()] });
        assert_eq!((slicers[0].columns, slicers[0].row_height, slicers[0].style.as_str()), (2, 241_300, "SlicerStyleLight2"));
        assert!(!slicers[0].descending);
        assert_eq!(slicers[1].source, SlicerSource::Table { table: "売上表".into(), field: "品名".into() });
        assert!(!slicers[1].show_caption);
        assert!(slicers[1].descending);
        assert_eq!(slicers[1].caption, "品名");
    }
}
