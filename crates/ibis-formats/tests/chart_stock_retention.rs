// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Stock chart retention and roundtrip tests for HLC and OHLC.
use ibis_doc_model::{
    ChartGroup, ChartType, Command, Document, Number, Presentation, ShapeKind,
};
use ibis_ooxml::{Package, SaveOptions};

fn stock_hlc_xml(drop_lines: bool, hi_low_lines: bool) -> String {
    let dl = if drop_lines { "<c:dropLines/>" } else { "" };
    let hl = if hi_low_lines { "<c:hiLowLines/>" } else { "" };
    format!(
        r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:stockChart><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>高値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>30</c:v></c:pt><c:pt idx="1"><c:v>35</c:v></c:pt></c:numLit></c:val></c:ser><c:ser><c:idx val="1"/><c:order val="1"/><c:tx><c:v>安値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>20</c:v></c:pt><c:pt idx="1"><c:v>22</c:v></c:pt></c:numLit></c:val></c:ser><c:ser><c:idx val="2"/><c:order val="2"/><c:tx><c:v>終値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>25</c:v></c:pt><c:pt idx="1"><c:v>31</c:v></c:pt></c:numLit></c:val></c:ser>{dl}{hl}<c:axId val="100000001"/><c:axId val="100000002"/></c:stockChart><c:catAx><c:axId val="100000001"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="100000002"/></c:catAx><c:valAx><c:axId val="100000002"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="100000001"/></c:valAx></c:plotArea><c:legend><c:legendPos val="b"/><c:overlay val="0"/></c:legend></c:chart></c:chartSpace>"#
    )
}

fn stock_ohlc_xml(drop_lines: bool, hi_low_lines: bool, up_down_bars: bool) -> String {
    let dl = if drop_lines { "<c:dropLines/>" } else { "" };
    let hl = if hi_low_lines { "<c:hiLowLines/>" } else { "" };
    let ud = if up_down_bars {
        "<c:upDownBars><c:gapWidth val=\"150\"/><c:upBars/><c:downBars/></c:upDownBars>"
    } else {
        ""
    };
    format!(
        r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:stockChart><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>始値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>22</c:v></c:pt><c:pt idx="1"><c:v>25</c:v></c:pt></c:numLit></c:val></c:ser><c:ser><c:idx val="1"/><c:order val="1"/><c:tx><c:v>高値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>30</c:v></c:pt><c:pt idx="1"><c:v>35</c:v></c:pt></c:numLit></c:val></c:ser><c:ser><c:idx val="2"/><c:order val="2"/><c:tx><c:v>安値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>20</c:v></c:pt><c:pt idx="1"><c:v>22</c:v></c:pt></c:numLit></c:val></c:ser><c:ser><c:idx val="3"/><c:order val="3"/><c:tx><c:v>終値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>25</c:v></c:pt><c:pt idx="1"><c:v>31</c:v></c:pt></c:numLit></c:val></c:ser>{dl}{hl}{ud}<c:axId val="100000001"/><c:axId val="100000002"/></c:stockChart><c:catAx><c:axId val="100000001"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="100000002"/></c:catAx><c:valAx><c:axId val="100000002"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="100000001"/></c:valAx></c:plotArea><c:legend><c:legendPos val="b"/><c:overlay val="0"/></c:legend></c:chart></c:chartSpace>"#
    )
}

fn stock_vhlc_xml(drop_lines: bool, hi_low_lines: bool) -> String {
    let dl = if drop_lines { "<c:dropLines/>" } else { "" };
    let hl = if hi_low_lines { "<c:hiLowLines/>" } else { "" };
    format!(
        r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:varyColors val="0"/><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>出来高</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>1000</c:v></c:pt><c:pt idx="1"><c:v>1500</c:v></c:pt></c:numLit></c:val></c:ser><c:gapWidth val="150"/><c:axId val="100000001"/><c:axId val="100000002"/></c:barChart><c:stockChart><c:ser><c:idx val="1"/><c:order val="1"/><c:tx><c:v>高値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>30</c:v></c:pt><c:pt idx="1"><c:v>35</c:v></c:pt></c:numLit></c:val></c:ser><c:ser><c:idx val="2"/><c:order val="2"/><c:tx><c:v>安値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>20</c:v></c:pt><c:pt idx="1"><c:v>22</c:v></c:pt></c:numLit></c:val></c:ser><c:ser><c:idx val="3"/><c:order val="3"/><c:tx><c:v>終値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>25</c:v></c:pt><c:pt idx="1"><c:v>31</c:v></c:pt></c:numLit></c:val></c:ser>{dl}{hl}<c:axId val="100000001"/><c:axId val="100000003"/></c:stockChart><c:catAx><c:axId val="100000001"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="100000002"/></c:catAx><c:valAx><c:axId val="100000002"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="100000001"/></c:valAx><c:valAx><c:axId val="100000003"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="r"/><c:crossAx val="100000001"/><c:crosses val="max"/></c:valAx></c:plotArea><c:legend><c:legendPos val="b"/><c:overlay val="0"/></c:legend></c:chart></c:chartSpace>"#
    )
}

fn stock_vohlc_xml(drop_lines: bool, hi_low_lines: bool, up_down_bars: bool) -> String {
    let dl = if drop_lines { "<c:dropLines/>" } else { "" };
    let hl = if hi_low_lines { "<c:hiLowLines/>" } else { "" };
    let ud = if up_down_bars {
        "<c:upDownBars><c:gapWidth val=\"150\"/><c:upBars/><c:downBars/></c:upDownBars>"
    } else {
        ""
    };
    format!(
        r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:varyColors val="0"/><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>出来高</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>1000</c:v></c:pt><c:pt idx="1"><c:v>1500</c:v></c:pt></c:numLit></c:val></c:ser><c:gapWidth val="150"/><c:axId val="100000001"/><c:axId val="100000002"/></c:barChart><c:stockChart><c:ser><c:idx val="1"/><c:order val="1"/><c:tx><c:v>始値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>22</c:v></c:pt><c:pt idx="1"><c:v>25</c:v></c:pt></c:numLit></c:val></c:ser><c:ser><c:idx val="2"/><c:order val="2"/><c:tx><c:v>高値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>30</c:v></c:pt><c:pt idx="1"><c:v>35</c:v></c:pt></c:numLit></c:val></c:ser><c:ser><c:idx val="3"/><c:order val="3"/><c:tx><c:v>安値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>20</c:v></c:pt><c:pt idx="1"><c:v>22</c:v></c:pt></c:numLit></c:val></c:ser><c:ser><c:idx val="4"/><c:order val="4"/><c:tx><c:v>終値</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>日1</c:v></c:pt><c:pt idx="1"><c:v>日2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>25</c:v></c:pt><c:pt idx="1"><c:v>31</c:v></c:pt></c:numLit></c:val></c:ser>{dl}{hl}{ud}<c:axId val="100000001"/><c:axId val="100000003"/></c:stockChart><c:catAx><c:axId val="100000001"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="100000002"/></c:catAx><c:valAx><c:axId val="100000002"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="100000001"/></c:valAx><c:valAx><c:axId val="100000003"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="r"/><c:crossAx val="100000001"/><c:crosses val="max"/></c:valAx></c:plotArea><c:legend><c:legendPos val="b"/><c:overlay val="0"/></c:legend></c:chart></c:chartSpace>"#
    )
}

fn fixture(xlsx: bool, xml: &str) -> (Vec<u8>, &'static str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let mut package = Package::open(std::fs::read(root.join(if xlsx { "xlsx-sales.xlsx" } else { "pptx-plan.pptx" })).unwrap()).unwrap();
    let part = if xlsx { "xl/charts/chart1.xml" } else { "ppt/charts/stock-retention.xml" };
    package.set_bytes(part, xml.as_bytes().to_vec());
    if !xlsx {
        package.set_override(part, "application/vnd.openxmlformats-officedocument.drawingml.chart+xml").unwrap();
        let slide = "ppt/slides/slide1.xml";
        let rid = package.add_relationship(slide, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart", "../charts/stock-retention.xml", false).unwrap();
        let xml = String::from_utf8(package.read(slide).unwrap()).unwrap();
        let frame = format!(r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="900" name="stock chart"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="1000000" y="1000000"/><a:ext cx="3000000" cy="2000000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="{rid}"/></a:graphicData></a:graphic></p:graphicFrame>"#);
        assert!(xml.contains("</p:spTree>"));
        package.set_bytes(slide, xml.replacen("</p:spTree>", &(frame + "</p:spTree>"), 1).into_bytes());
    }
    (package.save(SaveOptions::default()).unwrap(), part)
}

fn chart_group(p: &Presentation) -> &ChartGroup {
    p.slides
        .iter()
        .flat_map(|s| &s.shapes)
        .find_map(|s| match &s.kind {
            ShapeKind::Chart(c) => c.groups.iter().find(|g| matches!(g.kind, ChartType::Stock { .. })),
            _ => None,
        })
        .expect("Stock chart group")
}

#[test]
fn stock_chart_hlc_reading_and_retention() {
    let (data, _part) = fixture(true, &stock_hlc_xml(false, true));
    let opened = ibis_formats::open(data.clone()).unwrap();
    let g = chart_group(&opened.presentation);
    assert_eq!(
        g.kind,
        ChartType::Stock {
            up_down_bars: false,
            hi_low_lines: true,
            drop_lines: false,
        }
    );
    assert_eq!(g.series.len(), 3);
    assert_eq!(g.series[0].name, "高値");
    assert_eq!(g.series[1].name, "安値");
    assert_eq!(g.series[2].name, "終値");
    assert_eq!(g.series[0].values, vec![Some(Number(30.0)), Some(Number(35.0))]);

    // XLSX save preserves bytes when unchanged
    let saved = opened.source.save(&opened.presentation, SaveOptions::default()).unwrap();
    assert_eq!(saved, data);
}

#[test]
fn stock_chart_ohlc_reading_and_retention() {
    let (data, _part) = fixture(true, &stock_ohlc_xml(true, true, true));
    let opened = ibis_formats::open(data.clone()).unwrap();
    let g = chart_group(&opened.presentation);
    assert_eq!(
        g.kind,
        ChartType::Stock {
            up_down_bars: true,
            hi_low_lines: true,
            drop_lines: true,
        }
    );
    assert_eq!(g.series.len(), 4);
    assert_eq!(g.series[0].name, "始値");
    assert_eq!(g.series[1].name, "高値");
    assert_eq!(g.series[2].name, "安値");
    assert_eq!(g.series[3].name, "終値");

    let saved = opened.source.save(&opened.presentation, SaveOptions::default()).unwrap();
    assert_eq!(saved, data);
}

#[test]
fn stock_chart_settings_edit_and_save() {
    let (data, part) = fixture(true, &stock_hlc_xml(false, true));
    let opened = ibis_formats::open(data).unwrap();
    let mut doc = Document::new(opened.presentation.clone());
    let (slide_id, shape_id, mut modified) = doc
        .presentation()
        .slides
        .iter()
        .flat_map(|s| s.shapes.iter().map(move |sh| (s.id, sh.id, sh.kind.clone())))
        .find_map(|(s, id, kind)| match kind {
            ShapeKind::Chart(c) => Some((s, id, (*c).clone())),
            _ => None,
        })
        .unwrap();

    // Toggle drop_lines and up_down_bars
    modified.groups[0].kind = ChartType::Stock {
        up_down_bars: true,
        hi_low_lines: true,
        drop_lines: true,
    };

    doc.execute(vec![Command::SetShapeKind {
        slide: slide_id,
        shape: shape_id,
        kind: Box::new(ShapeKind::Chart(Box::new(modified))),
    }])
    .unwrap();

    let saved = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    let reloaded = ibis_formats::open(saved.clone()).unwrap();
    let g = chart_group(&reloaded.presentation);
    assert_eq!(
        g.kind,
        ChartType::Stock {
            up_down_bars: true,
            hi_low_lines: true,
            drop_lines: true,
        }
    );

    // Verify XML contains dropLines, hiLowLines, upDownBars
    let pkg = Package::open(saved).unwrap();
    let xml = String::from_utf8(pkg.read(part).unwrap()).unwrap();
    assert!(xml.contains("<c:dropLines"));
    assert!(xml.contains("<c:hiLowLines"));
    assert!(xml.contains("<c:upDownBars"));
}

fn get_chart(p: &Presentation) -> &ibis_doc_model::Chart {
    p.slides
        .iter()
        .flat_map(|s| &s.shapes)
        .find_map(|s| match &s.kind {
            ShapeKind::Chart(c) => Some(&**c),
            _ => None,
        })
        .expect("Chart")
}

#[test]
fn stock_chart_vhlc_reading_and_retention() {
    let (data, _part) = fixture(true, &stock_vhlc_xml(false, true));
    let opened = ibis_formats::open(data.clone()).unwrap();
    let c = get_chart(&opened.presentation);
    assert_eq!(c.groups.len(), 2);
    assert_eq!(
        c.groups[0].kind,
        ChartType::Bar {
            horizontal: false,
            gap_width: 150,
            overlap: 0,
        }
    );
    assert_eq!(c.groups[0].series.len(), 1);
    assert_eq!(c.groups[0].series[0].name, "出来高");
    assert_eq!(c.groups[0].series[0].values, vec![Some(Number(1000.0)), Some(Number(1500.0))]);

    assert_eq!(
        c.groups[1].kind,
        ChartType::Stock {
            up_down_bars: false,
            hi_low_lines: true,
            drop_lines: false,
        }
    );
    assert_eq!(c.groups[1].series.len(), 3);
    assert_eq!(c.groups[1].series[0].name, "高値");
    assert_eq!(c.groups[1].series[1].name, "安値");
    assert_eq!(c.groups[1].series[2].name, "終値");
    assert_eq!(c.groups[1].series[0].values, vec![Some(Number(30.0)), Some(Number(35.0))]);

    // Check axes: category axis, primary value axis (left), secondary value axis (right)
    assert_eq!(c.axes.len(), 3);
    assert_eq!(c.axes[0].kind, ibis_doc_model::AxisKind::Category);
    assert_eq!(c.axes[1].kind, ibis_doc_model::AxisKind::Value);
    assert_eq!(c.axes[1].side, ibis_doc_model::Side::Left);
    assert_eq!(c.axes[2].kind, ibis_doc_model::AxisKind::Value);
    assert_eq!(c.axes[2].side, ibis_doc_model::Side::Right);
    assert_eq!(c.axes[2].crosses, Some(ibis_doc_model::AxisCrosses::Maximum));

    // XLSX save preserves bytes when unchanged
    let saved = opened.source.save(&opened.presentation, SaveOptions::default()).unwrap();
    assert_eq!(saved, data);
}

#[test]
fn stock_chart_vohlc_reading_and_retention() {
    let (data, _part) = fixture(true, &stock_vohlc_xml(true, true, true));
    let opened = ibis_formats::open(data.clone()).unwrap();
    let c = get_chart(&opened.presentation);
    assert_eq!(c.groups.len(), 2);
    assert_eq!(
        c.groups[0].kind,
        ChartType::Bar {
            horizontal: false,
            gap_width: 150,
            overlap: 0,
        }
    );
    assert_eq!(c.groups[0].series.len(), 1);
    assert_eq!(c.groups[0].series[0].name, "出来高");

    assert_eq!(
        c.groups[1].kind,
        ChartType::Stock {
            up_down_bars: true,
            hi_low_lines: true,
            drop_lines: true,
        }
    );
    assert_eq!(c.groups[1].series.len(), 4);
    assert_eq!(c.groups[1].series[0].name, "始値");
    assert_eq!(c.groups[1].series[1].name, "高値");
    assert_eq!(c.groups[1].series[2].name, "安値");
    assert_eq!(c.groups[1].series[3].name, "終値");

    let saved = opened.source.save(&opened.presentation, SaveOptions::default()).unwrap();
    assert_eq!(saved, data);
}

#[test]
fn stock_chart_volume_settings_edit_and_save() {
    let (data, part) = fixture(true, &stock_vhlc_xml(false, true));
    let opened = ibis_formats::open(data).unwrap();
    let mut doc = Document::new(opened.presentation.clone());
    let (slide_id, shape_id, mut modified) = doc
        .presentation()
        .slides
        .iter()
        .flat_map(|s| s.shapes.iter().map(move |sh| (s.id, sh.id, sh.kind.clone())))
        .find_map(|(s, id, kind)| match kind {
            ShapeKind::Chart(c) => Some((s, id, (*c).clone())),
            _ => None,
        })
        .unwrap();

    // Toggle drop_lines and up_down_bars on the stock group (index 1)
    modified.groups[1].kind = ChartType::Stock {
        up_down_bars: true,
        hi_low_lines: true,
        drop_lines: true,
    };
    // Modify bar gap_width on the volume group (index 0)
    modified.groups[0].kind = ChartType::Bar {
        horizontal: false,
        gap_width: 200,
        overlap: 0,
    };

    doc.execute(vec![Command::SetShapeKind {
        slide: slide_id,
        shape: shape_id,
        kind: Box::new(ShapeKind::Chart(Box::new(modified))),
    }])
    .unwrap();

    let saved = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    let reloaded = ibis_formats::open(saved.clone()).unwrap();
    let c = get_chart(&reloaded.presentation);
    assert_eq!(
        c.groups[0].kind,
        ChartType::Bar {
            horizontal: false,
            gap_width: 200,
            overlap: 0,
        }
    );
    assert_eq!(
        c.groups[1].kind,
        ChartType::Stock {
            up_down_bars: true,
            hi_low_lines: true,
            drop_lines: true,
        }
    );

    // Verify XML contains dropLines, upDownBars, and gapWidth val="200"
    let pkg = Package::open(saved).unwrap();
    let xml = String::from_utf8(pkg.read(part).unwrap()).unwrap();
    assert!(xml.contains("<c:dropLines"));
    assert!(xml.contains("<c:upDownBars"));
    assert!(xml.contains("<c:gapWidth val=\"200\""));
}

