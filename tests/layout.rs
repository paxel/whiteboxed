use whiteboxed::geom::{Pos, Rect};
use whiteboxed::layout::{self, Layout, LineKind, MIN_H, PORT_SPACING};
use whiteboxed::model::{
    BlockId, BlockKind, BlockSpec, Cell, Direction, End, LineStyle, Placement, Project, Side,
};
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
    p.connect_new(
        api,
        Side::Bottom,
        &spec("Application Services", BlockKind::Component),
        Direction::Out,
        "use cases",
    )?;
    let (_, r) = p.connect_new(
        api,
        Side::Right,
        &spec("Telemetry", BlockKind::Component),
        Direction::Out,
        "tracing spans, request metrics and the /metrics endpoint for Prometheus",
    )?;
    p.set_relation_short(r, "telemetry")?;
    p.connect_new(
        api,
        Side::Left,
        &spec("Web UI", BlockKind::Ui),
        Direction::In,
        "fetch JSON, SSE/WS updates for the live dashboard",
    )?;
    let png = export::to_png(&scene::scene(&render(&p, None)), 1.0)?;
    std::fs::write("target/legend.png", png)?;
    Ok(())
}

fn line_of(
    l: &Layout,
    rel: whiteboxed::model::RelationId,
) -> Result<Vec<Pos>, Box<dyn std::error::Error>> {
    Ok(l.lines
        .iter()
        .find(|g| g.relation == rel)
        .ok_or("line")?
        .points
        .clone())
}

#[test]
fn facing_boxes_get_straight_lines() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &spec("A", BlockKind::Component))?;
    let (b, ab) = p.connect_new(
        a,
        Side::Right,
        &spec("B", BlockKind::Component),
        Direction::Out,
        "",
    )?;
    // A second line on A's right side, to a box further down.
    let (_, ac) = p.connect_new(
        a,
        Side::Right,
        &spec("C", BlockKind::Component),
        Direction::Out,
        "",
    )?;
    // And two lines between A and B: both straight, side by side.
    let ab2 = p.connect_existing(a, Side::Right, b, Direction::In, "", Placement::Move)?;
    let l = render(&p, None);
    for rel in [ab, ab2] {
        let pts = line_of(&l, rel)?;
        assert_eq!(pts.len(), 2, "{rel:?} is one straight segment: {pts:?}");
        assert!((pts[0].y - pts[1].y).abs() < 0.01);
    }
    let y1 = line_of(&l, ab)?[0].y;
    let y2 = line_of(&l, ab2)?[0].y;
    assert!((y1 - y2).abs() >= whiteboxed::layout::PORT_SPACING - 0.01);
    assert!(line_of(&l, ac)?.len() > 2, "C is below: that one bends");
    Ok(())
}

#[test]
fn frame_ends_line_up_with_the_box_that_takes_them() -> TestResult {
    let mut p = Project::new();
    let user = p.add_block(None, &spec("User", BlockKind::Person))?;
    let (shop, rel) = p.connect_new(
        user,
        Side::Right,
        &spec("Shop", BlockKind::Component),
        Direction::Out,
        "",
    )?;
    let ui = p.add_block(Some(shop), &spec("UI", BlockKind::Ui))?;
    p.connect_new(
        ui,
        Side::Bottom,
        &spec("Orders", BlockKind::Component),
        Direction::Out,
        "",
    )?;
    p.connect_new(
        ui,
        Side::Right,
        &spec("Search", BlockKind::Component),
        Direction::Out,
        "",
    )?;
    p.attach(rel, End::B, shop, ui, Side::Left)?;
    let l = render(&p, Some(shop));
    let pts = line_of(&l, rel)?;
    assert_eq!(pts.len(), 2, "straight from the frame into UI: {pts:?}");
    Ok(())
}

#[test]
fn labels_stay_inside_the_frame_and_the_picture() -> TestResult {
    let mut p = Project::new();
    p.set_label_limit(None);
    let svc = p.add_block(None, &spec("Service", BlockKind::Component))?;
    let long = "bind + group lookup against the corporate directory (LDAP/LDAPS) on every login";
    let (_, rel) = p.connect_new(
        svc,
        Side::Right,
        &spec("Directory", BlockKind::ExternalSystem),
        Direction::Out,
        long,
    )?;
    let auth = p.add_block(Some(svc), &spec("Auth", BlockKind::Component))?;
    p.attach(rel, End::A, svc, auth, Side::Right)?;
    let l = render(&p, Some(svc));
    let frame = l.frame.ok_or("frame")?;
    let s = scene::scene(&l);
    for shape in &s.shapes {
        if let whiteboxed::scene::Shape::Text {
            pos,
            text,
            size,
            align,
            ..
        } = shape
        {
            let w = whiteboxed::text::width(text, *size);
            let left = match align {
                whiteboxed::scene::Align::Left => pos.x,
                whiteboxed::scene::Align::Center => pos.x - w / 2.0,
                whiteboxed::scene::Align::Right => pos.x - w,
            };
            assert!(
                left >= s.bounds.min.x && left + w <= s.bounds.max.x,
                "{text} sticks out"
            );
            if text.starts_with("bind") || long.contains(text.as_str()) {
                assert!(
                    left >= frame.min.x && left + w <= frame.max.x,
                    "{text} crosses the frame"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn lines_avoid_crossings_when_they_can() -> TestResult {
    // Before crossings counted, these three lines crossed twice.
    let mut p = Project::new();
    let k = BlockKind::Component;
    let gateway = p.add_block(None, &spec("Gateway", k))?;
    let auth = p.add_block(None, &spec("Auth", k))?;
    let billing = p.add_block(None, &spec("Billing", k))?;
    let ledger = p.add_block(None, &spec("Ledger", k))?;
    p.move_block(gateway, Cell { col: 1, row: 1 })?;
    p.move_block(auth, Cell { col: 1, row: 0 })?;
    p.move_block(billing, Cell { col: 3, row: 2 })?;
    p.move_block(ledger, Cell { col: 2, row: 1 })?;
    p.connect_existing(
        auth,
        Side::Right,
        ledger,
        Direction::Out,
        "a",
        Placement::Keep,
    )?;
    p.connect_existing(
        gateway,
        Side::Right,
        billing,
        Direction::Out,
        "b",
        Placement::Keep,
    )?;
    p.connect_existing(
        billing,
        Side::Top,
        ledger,
        Direction::Out,
        "c",
        Placement::Keep,
    )?;
    let l = render(&p, None);
    assert_eq!(l.lines.len(), 3);
    assert_eq!(layout::crossings(&l), Vec::new());
    Ok(())
}

#[test]
fn unavoidable_crossings_get_a_line_jump() -> TestResult {
    // A box in the middle: the line from top to bottom has to cross the one from
    // left to right somewhere.
    let mut p = Project::new();
    let k = BlockKind::Component;
    let cells = [(0, 1), (2, 1), (1, 0), (1, 2), (1, 1)];
    let mut ids = Vec::new();
    for (i, (col, row)) in cells.into_iter().enumerate() {
        let id = p.add_block(None, &spec(&format!("B{i}"), k))?;
        p.move_block(id, Cell { col, row })?;
        ids.push(id);
    }
    p.connect_existing(
        ids[0],
        Side::Right,
        ids[1],
        Direction::Out,
        "lr",
        Placement::Keep,
    )?;
    p.connect_existing(
        ids[2],
        Side::Bottom,
        ids[3],
        Direction::Out,
        "tb",
        Placement::Keep,
    )?;
    for style in LineStyle::ALL {
        p.set_line_style(style);
        let l = render(&p, None);
        let crossings = layout::crossings(&l);
        assert!(!crossings.is_empty(), "{style:?}");
        let s = scene::scene(&l);
        for c in crossings {
            // The horizontal line passes over the crossing point in a small arc.
            let top = Pos::new(c.at.x, c.at.y - scene::JUMP_RADIUS);
            let jumped = s.shapes.iter().any(|shape| match shape {
                scene::Shape::Polyline { points, .. } => points.iter().any(|q| q.dist(top) < 0.5),
                _ => false,
            });
            assert!(jumped, "{style:?}: no jump at {:?}", c.at);
        }
    }
    Ok(())
}

#[test]
fn a_jump_bulges_over_the_crossing_in_either_direction() {
    let jumps = [Pos::new(50.0, 10.0)];
    for (from, to) in [(0.0, 100.0), (100.0, 0.0)] {
        let out = scene::with_jumps(vec![Pos::new(from, 10.0), Pos::new(to, 10.0)], &jumps);
        assert_eq!(out.first(), Some(&Pos::new(from, 10.0)));
        assert_eq!(out.last(), Some(&Pos::new(to, 10.0)));
        let min_y = out.iter().map(|p| p.y).fold(f32::MAX, f32::min);
        assert!((min_y - (10.0 - scene::JUMP_RADIUS)).abs() < 0.01);
        // Points keep moving from `from` toward `to`.
        let xs: Vec<f32> = out.iter().map(|p| p.x).collect();
        assert!(xs.windows(2).all(|w| (w[1] - w[0]) * (to - from) >= -0.01));
    }
}
