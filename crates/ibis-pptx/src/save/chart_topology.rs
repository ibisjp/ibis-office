// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Preserve chart XML while splitting/reordering groups and adding secondary axes.
use super::unsupported;
use crate::Error;
use ibis_doc_model::chart::{AxisSourceKind, Side};
use ibis_doc_model::{Chart, ChartType};
use ibis_ooxml::{NodeId, XmlDocument};

fn group_name(kind: ChartType, three_d: bool) -> &'static str {
    match kind {
        ChartType::Bar { .. } => {
            if three_d {
                "bar3DChart"
            } else {
                "barChart"
            }
        }
        ChartType::Line => {
            if three_d {
                "line3DChart"
            } else {
                "lineChart"
            }
        }
        ChartType::Area => {
            if three_d {
                "area3DChart"
            } else {
                "areaChart"
            }
        }
        ChartType::Pie { .. } => {
            if three_d {
                "pie3DChart"
            } else {
                "pieChart"
            }
        }
        ChartType::Doughnut { .. } => "doughnutChart",
        ChartType::Scatter { .. } => "scatterChart",
        ChartType::Bubble { .. } => "bubbleChart",
        ChartType::Radar { .. } => "radarChart",
        ChartType::OfPie { .. } => "ofPieChart",
        ChartType::Stock { .. } => "stockChart",
        ChartType::Surface { is_3d, .. } => {
            if is_3d {
                "surface3DChart"
            } else {
                "surfaceChart"
            }
        }
    }
}
// convert_group reads only the source family discriminant; these payloads are
// placeholders, never source defaults or values written to the output chart.
fn kind_of(name: &str) -> Option<ChartType> {
    Some(match name {
        "barChart" | "bar3DChart" => ChartType::Bar {
            horizontal: false,
            gap_width: 150,
            overlap: 0,
        },
        "lineChart" | "line3DChart" => ChartType::Line,
        "areaChart" | "area3DChart" => ChartType::Area,
        "pieChart" | "pie3DChart" => ChartType::Pie { first_angle: 0 },
        "doughnutChart" => ChartType::Doughnut {
            first_angle: 0,
            hole: 50,
        },
        "scatterChart" => ChartType::Scatter { smooth: false },
        "bubbleChart" => ChartType::Bubble { scale: 100 },
        "surfaceChart" => ChartType::Surface {
            is_3d: false,
            wireframe: false,
        },
        "surface3DChart" => ChartType::Surface {
            is_3d: true,
            wireframe: false,
        },
        _ => return None,
    })
}
fn value(doc: &mut XmlDocument, parent: NodeId, name: &str, val: &str) {
    let node = doc.child(parent, name).unwrap_or_else(|| {
        let prefix = doc
            .name(parent)
            .split_once(':')
            .map(|(p, _)| format!("{p}:"))
            .unwrap_or_default();
        doc.append_element(parent, &format!("{prefix}{name}"))
    });
    doc.set_attr(node, "val", val);
}
fn axis_name(kind: AxisSourceKind) -> &'static str {
    match kind {
        AxisSourceKind::Category => "catAx",
        AxisSourceKind::Value => "valAx",
        AxisSourceKind::Date => "dateAx",
        AxisSourceKind::Series => "serAx",
    }
}

/// Make the source's group topology match the model before its normal reference/style writer runs.
pub(crate) fn sync(doc: &mut XmlDocument, after: &Chart) -> Result<(), Error> {
    sync_impl(doc, after, None)?;
    // Conversion resets family-specific children (for example line markers to none).
    // The topology baseline adopts the new groups, so write their appearance here before
    // ordinary chart/axis settings compare against that baseline.
    let root = doc
        .root_element()
        .ok_or_else(|| unsupported("グラフのXML"))?;
    let plot = doc
        .path(root, &["chart", "plotArea"])
        .ok_or_else(|| unsupported("グラフのプロット領域"))?;
    let a = doc
        .lookup_prefix(root, super::pptx_chart::DRAWING_NS)
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| {
            doc.set_attr(root, "xmlns:a", super::pptx_chart::DRAWING_NS);
            "a".into()
        });
    let nodes = doc
        .elements(plot)
        .filter(|&n| doc.local_name(n).ends_with("Chart"))
        .flat_map(|g| {
            doc.elements(g)
                .filter(|&s| doc.local_name(s) == "ser")
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    for (node, (kind,series)) in nodes
        .into_iter()
        .zip(after.groups.iter().flat_map(|g| g.series.iter().map(move|s|(g.kind,s))))
    {
        let c = doc.name(node).split_once(':').map(|(p,_)|format!("{p}:" )).unwrap_or_default();
        super::pptx_chart::write_series_fill(doc, node, &series.fill, &c, &a)?;
        super::pptx_chart::write_series_stroke(doc, node, series.stroke.as_ref(), &c, &a)?;
        super::pptx_chart::write_series_marker(doc, node, if kind.supports_markers(){series.marker.as_ref()}else{None}, &c, &a)?;
        super::pptx_chart::write_labels(doc, node, None, series.labels.as_ref())?;
        super::pptx_chart::write_trendline(doc, node, series.trendline.as_ref())?;
        super::pptx_chart::write_error_bars(doc, node, series.error_bars.as_ref())?;
    }
    Ok(())
}
pub(crate) fn sync_series(
    doc: &mut XmlDocument,
    after: &Chart,
    refs: &[super::pptx_chart_data::SeriesRefs],
) -> Result<(), Error> {
    sync_impl(doc, after, Some(refs))
}
fn same_ref(a: &str, b: &str) -> bool {
    matches!((ibis_ref(a), ibis_ref(b)), (Some(a), Some(b)) if a == b)
}
fn ibis_ref(value: &str) -> Option<(String, ibis_doc_model::CellRange)> {
    super::pptx_chart_data::parse_ref(value).map(|(s, r)| (s.to_lowercase(), r))
}
fn sync_impl(
    doc: &mut XmlDocument,
    after: &Chart,
    refs: Option<&[super::pptx_chart_data::SeriesRefs]>,
) -> Result<(), Error> {
    // A changed crossAx must name an axis in this chart. Never serialize a
    // dangling link; unchanged foreign source data bypasses topology rewriting.
    if after.axes.iter().any(|axis|axis.cross_axis_id.is_some_and(|id|!after.axes.iter().any(|other|other.id==id))) {
        return Err(unsupported("chart axis crossing target"));
    }

    let root = doc
        .root_element()
        .ok_or_else(|| unsupported("グラフのXML"))?;
    let plot = doc
        .path(root, &["chart", "plotArea"])
        .ok_or_else(|| unsupported("グラフのプロット領域"))?;
    let groups = doc
        .elements(plot)
        .filter(|&n| doc.local_name(n).ends_with("Chart"))
        .collect::<Vec<_>>();
    if groups.is_empty() {
        return Err(unsupported("グラフの系列グループ"));
    }
    let same = groups.len() == after.groups.len()
        && groups.iter().zip(&after.groups).all(|(&node, g)| {
            doc.local_name(node) == group_name(g.kind, g.three_d.is_some())
                && doc
                    .elements(node)
                    .filter(|&n| doc.local_name(n) == "axId")
                    .filter_map(|n| doc.attr(n, "val").and_then(|v| v.parse::<u32>().ok()))
                    .collect::<Vec<_>>()
                    == g.axes
        });
    let same_order = refs.is_none_or(|refs| {
        let nodes = groups
            .iter()
            .flat_map(|&g| {
                doc.elements(g)
                    .filter(|&n| doc.local_name(n) == "ser")
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        nodes.len() == refs.len()
            && nodes.iter().zip(refs).all(|(&node, r)| {
                r.values.as_ref().is_none_or(|wanted| {
                    doc.child(node, "val")
                        .or_else(|| doc.child(node, "yVal"))
                        .is_some_and(|v| {
                            doc.descendants(v).iter().any(|&n| {
                                doc.local_name(n) == "f" && same_ref(&doc.text(n), wanted)
                            })
                        })
                })
            })
    });
    if !same || !same_order {
        let mut pool = groups
            .iter()
            .flat_map(|&g| {
                doc.elements(g)
                    .filter(|&s| doc.local_name(s) == "ser")
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let mut next_idx = pool
            .iter()
            .filter_map(|&ser| {
                doc.child(ser, "idx")
                    .and_then(|i| doc.attr(i, "val"))
                    .and_then(|s| s.parse::<u32>().ok())
            })
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let mut reordered = Vec::new();
        let mut ordinal = 0;
        for g in &after.groups {
            let want = group_name(g.kind, g.three_d.is_some());
            let source = groups
                .iter()
                .copied()
                .find(|&n| doc.local_name(n) == want)
                .unwrap_or(groups[0]);
            let node = doc.deep_clone(source);
            for child in doc
                .elements(node)
                .filter(|&n| matches!(doc.local_name(n), "ser" | "axId"))
                .collect::<Vec<_>>()
            {
                doc.detach(child);
            }
            for series in &g.series {
                let named = |node| {
                    doc.child(node, "tx").is_some_and(|tx| {
                        doc.descendants(tx)
                            .iter()
                            .any(|&n| doc.local_name(n) == "v" && doc.text(n) == series.name)
                    })
                };
                let candidates = refs
                    .and_then(|refs| refs.get(ordinal))
                    .and_then(|r| r.values.as_ref())
                    .map(|wanted| {
                        pool.iter()
                            .enumerate()
                            .filter(|(_, node)| {
                                doc.child(**node, "val")
                                    .or_else(|| doc.child(**node, "yVal"))
                                    .is_some_and(|v| {
                                        doc.descendants(v).iter().any(|&n| {
                                            doc.local_name(n) == "f"
                                                && same_ref(&doc.text(n), wanted)
                                        })
                                    })
                            })
                            .map(|(i, _)| i)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let named_candidates = if candidates.is_empty() {
                    pool.iter()
                        .enumerate()
                        .filter(|(_, node)| named(**node))
                        .map(|(i, _)| i)
                        .collect::<Vec<_>>()
                } else {
                    candidates
                        .iter()
                        .copied()
                        .filter(|&i| named(pool[i]))
                        .collect::<Vec<_>>()
                };
                let found = if candidates.len() == 1 {
                    candidates[0]
                } else if named_candidates.len() == 1 {
                    named_candidates[0]
                } else if candidates.len() > 1 || named_candidates.len() > 1 {
                    return Err(unsupported(
                        "元の系列を一意に識別できないグラフのグループ変更",
                    ));
                } else {
                    0
                };
                ordinal += 1;
                let copied_new = pool.is_empty();
                let source_ser = if copied_new {
                    doc.elements(source)
                        .find(|&s| doc.local_name(s) == "ser")
                        .ok_or_else(|| unsupported("グラフの系列"))?
                } else {
                    pool.remove(found)
                };
                let ser = doc.deep_clone(source_ser);
                value(doc, ser, "order", &(ordinal - 1).to_string());
                if copied_new {
                    value(doc, ser, "idx", &next_idx.to_string());
                    next_idx = next_idx.saturating_add(1);
                }

                // Schema order places series before labels/options/axis references.
                let before = doc.elements(node).find(|&n| {
                    !matches!(
                        doc.local_name(n),
                        "barDir"
                            | "ser"
                            | "grouping"
                            | "varyColors"
                            | "scatterStyle"
                            | "wireframe"
                            | "ofPieType"
                    )
                });
                if let Some(before) = before {
                    doc.insert_before(before, ser)
                } else {
                    let at = doc.children(node).len();
                    doc.insert(node, at, ser)
                }
            }
            if doc.local_name(node) != want {
                let mut old = g.clone();
                old.kind = kind_of(doc.local_name(source))
                    .ok_or_else(|| unsupported("異なる系統のグラフへの変更"))?;
                super::pptx_chart_type::convert_group(doc, plot, node, &old, g)?;
            }
            for id in &g.axes {
                let prefix = doc
                    .name(node)
                    .split_once(':')
                    .map(|(p, _)| format!("{p}:"))
                    .unwrap_or_default();
                let axis = doc.append_element(node, &format!("{prefix}axId"));
                doc.set_attr(axis, "val", &id.to_string());
                if let Some(ext) = doc.child(node, "extLst") {
                    doc.detach(axis);
                    doc.insert_before(ext, axis);
                }
            }
            reordered.push(node);
        }
        let first = groups[0];
        for node in reordered {
            doc.insert_before(first, node);
        }
        for node in groups {
            doc.detach(node);
        }
    }
    let old_axes = doc
        .elements(plot)
        .filter(|&n| matches!(doc.local_name(n), "catAx" | "dateAx" | "valAx" | "serAx"))
        .collect::<Vec<_>>();
    let mut keep = Vec::new();
    for axis in &after.axes {
        let id = axis.source_id.unwrap_or(axis.id);
        let existing = old_axes.iter().copied().find(|&n| {
            doc.child(n, "axId")
                .and_then(|c| doc.attr(c, "val"))
                .and_then(|v| v.parse::<u32>().ok())
                == Some(id)
        });
        let node = match existing {
            Some(n) => n,
            None => {
                let source = old_axes
                    .iter()
                    .copied()
                    .find(|&n| doc.local_name(n) == axis_name(axis.source_kind))
                    .ok_or_else(|| unsupported("グラフの軸の追加"))?;
                let n = doc.deep_clone(source);
                if let Some(ext) = doc.child(plot, "extLst") {
                    doc.insert_before(ext, n)
                } else {
                    let at = doc.children(plot).len();
                    doc.insert(plot, at, n)
                }
                n
            }
        };
        value(doc, node, "axId", &id.to_string());
        value(
            doc,
            node,
            "axPos",
            match axis.side {
                Side::Left => "l",
                Side::Right => "r",
                Side::Top => "t",
                Side::Bottom => "b",
            },
        );
        if let Some(cross) = axis.cross_axis_id {
            value(doc, node, "crossAx", &cross.to_string());
        }
        keep.push(node);
    }
    for node in old_axes {
        if !keep.contains(&node) {
            doc.detach(node);
        }
    }
    Ok(())
}

pub(crate) fn differs(old: &Chart, new: &Chart) -> bool {
    old.axes.len() != new.axes.len()
        || old
            .axes
            .iter()
            .zip(&new.axes)
            .any(|(a, b)| a.id != b.id || a.side != b.side || a.cross_axis_id != b.cross_axis_id)
        || old.groups.len() != new.groups.len()
        || old.groups.iter().zip(&new.groups).any(|(a, b)| {
            a.axes != b.axes || std::mem::discriminant(&a.kind) != std::mem::discriminant(&b.kind)
        })
}
/// A source baseline for the topology just installed; ordinary settings still get their delta written.
pub(crate) fn baseline(old: &Chart, new: &Chart) -> Chart {
    let mut base = old.clone();
    base.groups = new.groups.clone();
    base.axes = new
        .axes
        .iter()
        .map(|axis| {
            let mut a = old
                .axes
                .iter()
                .find(|a| a.id == axis.id)
                .or_else(|| old.axes.iter().find(|a| a.kind == axis.kind))
                .cloned()
                .unwrap_or_else(|| axis.clone());
            a.id = axis.id;
            a.source_id = axis.source_id;
            a.side = axis.side;
            a.cross_axis_id = axis.cross_axis_id;
            a
        })
        .collect();
    base
}

#[cfg(test)]
#[path = "../tests/chart_topology.rs"]
mod tests;
