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
