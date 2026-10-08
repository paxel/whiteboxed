use whiteboxed::geom::{Pos, Rect};
use whiteboxed::layout::{self, Layout, LineKind, MIN_H, PORT_SPACING};
use whiteboxed::model::{BlockId, BlockKind, BlockSpec, Direction, End, Placement, Project, Side};
use whiteboxed::view::{ViewEnd, dangling_count, diagram_view};
use whiteboxed::{export, scene};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn spec(name: &str, kind: BlockKind) -> BlockSpec {
    BlockSpec::new(name, kind)
}

/// A context view plus the whitebox of "Shop".
fn sample() -> Result<(Project, BlockId), Box<dyn std::error::Error>> {
    let mut p = Project::new();
    let user = p.add_block(None, &spec("Customer", BlockKind::Person))?;
    let (shop, buy) = p.connect_new(
        user,
        Side::Right,
        &spec("Web Shop", BlockKind::Component).tagged("core"),
        Direction::Out,
        "orders via browser",
    )?;
    let (pay, _) = p.connect_new(
        shop,
        Side::Right,
        &spec("Payment Provider", BlockKind::ExternalSystem),
        Direction::Bi,
        "REST",
    )?;
    p.connect_new(
        shop,
        Side::Bottom,
        &spec("Warehouse", BlockKind::ExternalSystem),
        Direction::Out,
        "shipping orders",
    )?;
    p.connect_existing(
        user,
        Side::Top,
        pay,
        Direction::In,
        "receipt",
        Placement::Keep,
    )?;
    p.add_stub(shop, Side::Top, Direction::Out, "metrics")?;

    let ui = p.add_block(Some(shop), &spec("Storefront", BlockKind::Ui))?;
    let (orders, _) = p.connect_new(
        ui,
        Side::Right,
        &spec("Orders", BlockKind::Component).tagged("core"),
        Direction::Out,
        "place order",
    )?;
    p.connect_new(
        orders,
        Side::Bottom,
        &spec("Order DB", BlockKind::Database),
        Direction::Out,
        "SQL",
    )?;
    p.connect_new(
        orders,
        Side::Right,
        &spec("Events", BlockKind::Queue),
        Direction::Out,
        "publish",
    )?;
    p.connect_new(
        ui,
        Side::Bottom,
        &spec("Session Cache", BlockKind::Cache),
        Direction::Bi,
        "",
    )?;
    p.connect_new(
        orders,
        Side::Top,
        &spec("Invoices", BlockKind::FileStorage),
        Direction::Out,
        "PDF",
    )?;
    p.attach(buy, End::B, shop, ui, Side::Left)?;
    Ok((p, shop))
}

fn render(p: &Project, diagram: Option<BlockId>) -> Layout {
    layout::layout(p, &diagram_view(p, diagram))
}

/// Segment strictly crosses the interior of the rect (touching the border is fine).
fn crosses(a: Pos, b: Pos, r: &Rect) -> bool {
    let inner = r.expand(-1.0);
    let steps = 50;
    (1..steps).any(|i| {
        let t = i as f32 / steps as f32;
        inner.contains(Pos::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t))
    })
}

#[test]
fn same_model_gives_the_same_layout() -> TestResult {
    let (p, shop) = sample()?;
    assert_eq!(render(&p, None), render(&p.clone(), None));
    assert_eq!(render(&p, Some(shop)), render(&p.clone(), Some(shop)));
    let back = whiteboxed::persist::from_yaml(&whiteboxed::persist::to_yaml(&p)?)?;
    assert_eq!(render(&back, Some(shop)), render(&p, Some(shop)));
    Ok(())
}

#[test]
fn routed_lines_are_orthogonal_and_never_cross_boxes() -> TestResult {
    let (p, shop) = sample()?;
    for diagram in [None, Some(shop)] {
        let l = render(&p, diagram);
        for line in l.lines.iter().filter(|l| l.kind == LineKind::Routed) {
            for w in line.points.windows(2) {
                let straight = (w[0].x - w[1].x).abs() < 0.01 || (w[0].y - w[1].y).abs() < 0.01;
                assert!(straight, "diagonal segment in {:?}", line.relation);
                for b in &l.blocks {
                    assert!(
                        !crosses(w[0], w[1], &b.rect),
                        "line {:?} crosses {}",
                        line.relation,
                        b.name
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn a_side_grows_with_its_relations() -> TestResult {
    let mut p = Project::new();
    let hub = p.add_block(None, &spec("Hub", BlockKind::Component))?;
    let before = render(&p, None).block(hub).ok_or("hub")?.rect;
    assert_eq!(before.height(), MIN_H);
    for i in 0..5 {
        p.connect_new(
            hub,
            Side::Right,
            &spec(&format!("S{i}"), BlockKind::Component),
            Direction::Out,
            "",
        )?;
    }
    let after = render(&p, None).block(hub).ok_or("hub")?.rect;
    assert_eq!(after.height(), 6.0 * PORT_SPACING);
    assert_eq!(after.width(), before.width());
    Ok(())
}

#[test]
fn whitebox_shows_inherited_ends_on_the_frame() -> TestResult {
    let (p, shop) = sample()?;
    let view = diagram_view(&p, Some(shop));
    // Customer->Shop attached to Storefront; Shop->Payment, Shop->Warehouse and the
    // metrics stub are not attached yet.
    assert_eq!(dangling_count(&view), 3);
    let attached = view
        .lines
        .iter()
        .find(|l| matches!(&l.a, ViewEnd::Frame { partner, .. } if partner == "Customer"))
        .ok_or("customer line")?;
    assert!(matches!(attached.b, ViewEnd::Block { .. }));
    let l = render(&p, Some(shop));
    assert!(l.frame.is_some());
    assert!(l.lines.iter().any(|g| g.leaves_open()));
    Ok(())
}

#[test]
fn png_export_renders_text() -> TestResult {
    let (p, shop) = sample()?;
    let s = scene::scene(&render(&p, Some(shop)));
    let svg = export::to_svg(&s);
    assert!(svg.contains("Storefront"));
    let png = export::to_png(&s, 1.0)?;
    assert_eq!(&png[1..4], b"PNG");
    // Text must be drawn: an empty diagram with only a name differs from one without.
    let mut q = Project::new();
    q.add_block(None, &spec("Only", BlockKind::Component))?;
    let with_text = export::to_png(&scene::scene(&render(&q, None)), 1.0)?;
    q.edit_block(
        *q.blocks.keys().next().ok_or("block")?,
        &spec("O", BlockKind::Component),
    )?;
    let shorter = export::to_png(&scene::scene(&render(&q, None)), 1.0)?;
    assert_ne!(with_text, shorter);
    Ok(())
}

/// Writes PNGs of the sample into `target/` for a human to look at.
#[test]
#[ignore]
fn render_sample_pngs() -> TestResult {
    let (p, shop) = sample()?;
    for (name, diagram) in [("context", None), ("shop", Some(shop))] {
        let png = export::to_png(&scene::scene(&render(&p, diagram)), 1.5)?;
        std::fs::write(format!("target/sample-{name}.png"), png)?;
    }
    Ok(())
}

#[test]
fn png_export_stays_within_the_size_cap() -> TestResult {
    let mut p = Project::new();
    let mut prev = p.add_block(None, &spec("B0", BlockKind::Component))?;
    for i in 1..60 {
        let (next, _) = p.connect_new(
            prev,
            Side::Right,
            &spec(&format!("B{i}"), BlockKind::Component),
            Direction::Out,
            "",
        )?;
        prev = next;
    }
    let png = export::to_png(&scene::scene(&render(&p, None)), 2.0)?;
    let w = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    assert!(w as f32 <= export::MAX_PNG_SIDE, "width {w}");
    assert!(w > 4000, "still large: {w}");
    Ok(())
}

#[test]
fn line_styles_round_the_bends_but_keep_the_ends() {
    use whiteboxed::model::LineStyle;
    use whiteboxed::scene::shaped;
    let route = [
        Pos::new(0.0, 0.0),
        Pos::new(100.0, 0.0),
        Pos::new(100.0, 100.0),
        Pos::new(104.0, 100.0),
        Pos::new(104.0, 200.0),
    ];
    assert_eq!(shaped(&route, LineStyle::Square), route.to_vec());
    for style in [LineStyle::Round6, LineStyle::Round12, LineStyle::Curved] {
        let out = shaped(&route, style);
        assert!(out.len() > route.len(), "{style:?} adds points");
        assert_eq!(out.first(), route.first());
        assert_eq!(out.last(), route.last());
        // The corner itself is cut off.
        assert!(!out.contains(&Pos::new(100.0, 0.0)), "{style:?}");
        // The last segment still points straight down, so the arrowhead does too.
        let n = out.len();
        assert!((out[n - 1].x - out[n - 2].x).abs() < 0.01);
    }
    // A 12 px radius cuts 12 px from a long segment, a curve reaches its middle.
    let round = shaped(&route, LineStyle::Round12);
    assert!(round.contains(&Pos::new(88.0, 0.0)));
    let curve = shaped(&route, LineStyle::Curved);
    assert!(curve.contains(&Pos::new(50.0, 0.0)));
    // The 4 px step only gets half its length as radius: it stays a step.
    assert!(round.iter().all(|p| p.y <= 100.0 || p.x >= 102.0));
}

#[test]
fn long_texts_become_numbers_and_short_labels_win() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &spec("A", BlockKind::Component))?;
    let long = "OTLP traces (gRPC) / Prometheus scrape /metrics";
    let (_, r1) = p.connect_new(
        a,
        Side::Right,
        &spec("B", BlockKind::Component),
        Direction::Out,
        long,
    )?;
    let (_, r2) = p.connect_new(
        a,
        Side::Bottom,
        &spec("C", BlockKind::Component),
        Direction::Out,
        "REST",
    )?;
    let (_, r3) = p.connect_new(
        a,
        Side::Left,
        &spec("D", BlockKind::Component),
        Direction::Out,
        long,
    )?;
    p.set_relation_short(r3, "telemetry")?;
    let l = render(&p, None);
    let shown = |r| {
        l.lines
            .iter()
            .find(|g| g.relation == r)
            .map(|g| g.text.clone())
    };
    assert_eq!(shown(r1).as_deref(), Some("[1]"));
    assert_eq!(shown(r2).as_deref(), Some("REST"));
    assert_eq!(shown(r3).as_deref(), Some("telemetry"));
    let keys: Vec<_> = l.legend.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys, vec!["[1]", "telemetry"]);
    assert_eq!(l.legend[0].lines.join(" "), long);
    // The legend sits below the diagram, inside the image.
    let at = l.legend_at.ok_or("legend position")?;
    let grid_bottom = l.cells.iter().map(|(_, r)| r.max.y).fold(0.0, f32::max);
    assert!(at.y > grid_bottom);
    assert!(l.bounds.max.y > at.y + 2.0 * whiteboxed::layout::LEGEND_LINE);
    // Off: full texts everywhere; 0: every text is a number.
    p.set_label_limit(None);
    let l = render(&p, None);
    assert!(l.lines.iter().any(|g| g.text == long));
    assert_eq!(l.legend.len(), 1, "only the short label is explained");
    p.set_label_limit(Some(0));
    let l = render(&p, None);
    let rest = l.lines.iter().find(|g| g.relation == r2).ok_or("rest")?;
    assert!(rest.text.starts_with('['));
    assert_eq!(rest.full_text, "REST");
    Ok(())
}

#[test]
fn long_legend_entries_wrap_to_the_diagram_width() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &spec("A", BlockKind::Component))?;
    let essay = "word ".repeat(400);
    p.connect_new(
        a,
        Side::Right,
        &spec("B", BlockKind::Component),
        Direction::Out,
        &essay,
    )?;
    let l = render(&p, None);
    let entry = l.legend.first().ok_or("entry")?;
    assert!(entry.lines.len() > 3);
    let widest = entry
        .lines
        .iter()
        .map(|t| whiteboxed::text::width(t, whiteboxed::layout::LABEL_SIZE))
        .fold(0.0, f32::max);
    assert!(widest <= l.bounds.width());
    Ok(())
}

/// Writes a diagram with shortened texts and a legend to `target/legend.png`.
#[test]
#[ignore]
fn render_legend_png() -> TestResult {
    let mut p = Project::new();
    let api = p.add_block(None, &spec("HTTP API", BlockKind::Component))?;
    p.connect_new(api, Side::Bottom, &spec("Application Services", BlockKind::Component), Direction::Out, "use cases")?;
    let (_, r) = p.connect_new(api, Side::Right, &spec("Telemetry", BlockKind::Component), Direction::Out, "tracing spans, request metrics and the /metrics endpoint for Prometheus")?;
    p.set_relation_short(r, "telemetry")?;
    p.connect_new(api, Side::Left, &spec("Web UI", BlockKind::Ui), Direction::In, "fetch JSON, SSE/WS updates for the live dashboard")?;
    let png = export::to_png(&scene::scene(&render(&p, None)), 1.0)?;
    std::fs::write("target/legend.png", png)?;
    Ok(())
}
