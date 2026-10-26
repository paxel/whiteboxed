use whiteboxed::geom::{Pos, Rect};
use whiteboxed::layout::{self, Layout, LineKind, MIN_H, PORT_SPACING};
use whiteboxed::model::{
    BlockId, BlockKind, BlockSpec, Cell, Direction, End, LineStyle, Placement, Project, RelationId,
    Side,
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
fn the_second_relation_on_a_side_already_grows_the_box() -> TestResult {
    let mut p = Project::new();
    let hub = p.add_block(None, &spec("Hub", BlockKind::Component))?;
    let mut heights = Vec::new();
    for i in 0..2 {
        p.connect_new(
            hub,
            Side::Left,
            &spec(&format!("S{i}"), BlockKind::Component),
            Direction::Out,
            "",
        )?;
        heights.push(render(&p, None).block(hub).ok_or("hub")?.rect.height());
    }
    assert_eq!(heights[0], MIN_H, "one relation keeps the minimum size");
    assert!(
        heights[1] > MIN_H,
        "two relations grow the side: {heights:?}"
    );
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
    // And two lines between A and B, drawn separately: both straight, side by side.
    p.set_bundle(false);
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

#[test]
fn labels_move_off_crossings() -> TestResult {
    // A box in the middle: the line from top to bottom has to cross the one from
    // left to right somewhere, and the labels keep away from that point.
    let mut p = Project::new();
    let k = BlockKind::Component;
    let mut ids = Vec::new();
    for (i, (col, row)) in [(0, 1), (2, 1), (1, 0), (1, 2), (1, 1)]
        .into_iter()
        .enumerate()
    {
        let id = p.add_block(None, &spec(&format!("B{i}"), k))?;
        p.move_block(id, Cell { col, row })?;
        ids.push(id);
    }
    p.connect_existing(
        ids[0],
        Side::Right,
        ids[1],
        Direction::Out,
        "left to right",
        Placement::Keep,
    )?;
    p.connect_existing(
        ids[2],
        Side::Bottom,
        ids[3],
        Direction::Out,
        "top to bottom",
        Placement::Keep,
    )?;
    let l = render(&p, None);
    let crossings = layout::crossings(&l);
    assert!(!crossings.is_empty());
    for c in crossings {
        for line in &l.lines {
            assert!(
                line.label_at.dist(c.at) > 20.0,
                "label of {} sits on the crossing at {:?}",
                line.text,
                c.at
            );
        }
    }
    Ok(())
}

#[test]
fn a_line_without_direction_has_no_arrowheads() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &spec("A", BlockKind::Component))?;
    p.connect_new(
        a,
        Side::Right,
        &spec("B", BlockKind::Component),
        Direction::Undirected,
        "",
    )?;
    let l = render(&p, None);
    let line = l.lines.first().ok_or("line")?;
    assert!(!line.arrow_start && !line.arrow_end);
    let arrows = scene::scene(&l)
        .shapes
        .iter()
        .filter(|s| matches!(s, scene::Shape::Polygon { .. }))
        .count();
    assert_eq!(arrows, 0);
    Ok(())
}

#[test]
fn bands_run_across_the_bottom_inside_the_frame() -> TestResult {
    let (mut p, shop) = sample()?;
    let ctx_before = render(&p, None).bounds;
    let log = p.add_block(None, &spec("Logging", BlockKind::Component).as_band())?;
    let sec = p.add_block(
        Some(shop),
        &spec("Security", BlockKind::Component).as_band(),
    )?;
    let audit = p.add_block(Some(shop), &spec("Audit", BlockKind::Component).as_band())?;

    let ctx = render(&p, None);
    let band = ctx.block(log).ok_or("logging")?;
    assert!(band.band);
    assert!(ctx.bounds.height() > ctx_before.height());
    for b in ctx.blocks.iter().filter(|b| !b.band) {
        assert!(
            b.rect.max.y < band.rect.min.y,
            "{} is below the band",
            b.name
        );
    }
    assert!(band.rect.width() >= ctx_before.width() - layout::GAP - 0.01);

    let wb = render(&p, Some(shop));
    let frame = wb.frame.ok_or("frame")?;
    let (s, a) = (
        wb.block(sec).ok_or("security")?.rect,
        wb.block(audit).ok_or("audit")?.rect,
    );
    assert!(frame.contains(s.min) && frame.contains(a.max));
    // In the order they were added, one below the other.
    assert!(s.max.y < a.min.y);
    // A click on a band's border selects it; it offers no line to start.
    let edge = Pos::new(s.center().x, s.min.y + 1.0);
    assert_eq!(
        whiteboxed::hit::hit(&p, &wb, edge, 1.0),
        whiteboxed::hit::Hit::Block(sec)
    );
    Ok(())
}

#[test]
fn a_fanned_out_relation_enters_through_one_port_with_one_label() -> TestResult {
    let mut p = Project::new();
    let user = p.add_block(None, &spec("Customer", BlockKind::Person))?;
    let (shop, rel) = p.connect_new(
        user,
        Side::Right,
        &spec("Shop", BlockKind::Component),
        Direction::Out,
        "orders",
    )?;
    let ui = p.add_block(Some(shop), &spec("UI", BlockKind::Component))?;
    let (api, _) = p.connect_new(
        ui,
        Side::Bottom,
        &spec("API", BlockKind::Component),
        Direction::Out,
        "calls",
    )?;
    p.attach(rel, End::B, shop, ui, Side::Left)?;
    p.attach(rel, End::B, shop, api, Side::Left)?;
    let l = render(&p, Some(shop));
    let branches: Vec<&layout::LineGeom> = l.lines.iter().filter(|g| g.relation == rel).collect();
    assert_eq!(branches.len(), 2);
    let ports: Vec<Pos> = branches
        .iter()
        .filter_map(|g| g.frame_port.as_ref().map(|(p, _, _)| *p))
        .collect();
    assert_eq!(ports.len(), 2);
    assert!(ports[0].dist(ports[1]) < 0.01, "one frame port: {ports:?}");
    let labels: Vec<&str> = branches
        .iter()
        .map(|g| g.text.as_str())
        .filter(|t| !t.is_empty())
        .collect();
    assert_eq!(labels, vec!["orders"]);
    let landings: Vec<Option<BlockId>> = branches.iter().map(|g| g.landing).collect();
    assert_eq!(landings, vec![Some(ui), Some(api)]);
    Ok(())
}

#[test]
fn label_rects_overlap_and_fit_like_the_drawing() -> TestResult {
    let a = Rect::from_min_size(Pos::new(0.0, 0.0), 10.0, 10.0);
    assert!(a.overlaps(&Rect::from_min_size(Pos::new(5.0, 5.0), 10.0, 10.0)));
    assert!(
        !a.overlaps(&Rect::from_min_size(Pos::new(10.0, 0.0), 10.0, 10.0)),
        "edges touch only"
    );
    assert!(a.holds(&Rect::from_min_size(Pos::new(1.0, 1.0), 2.0, 2.0)));
    assert!(!a.holds(&Rect::from_min_size(Pos::new(9.0, 1.0), 2.0, 2.0)));
    let (p, _) = sample()?;
    let l = render(&p, None);
    let labels = scene::label_rects(&l);
    let texts = l.lines.iter().filter(|g| !g.text.is_empty()).count();
    assert_eq!(labels.len(), texts);
    let area = scene::label_area(&l);
    assert!(labels.iter().all(|(_, r)| area.holds(r)));
    Ok(())
}

#[test]
fn facing_boxes_across_empty_cells_get_a_straight_line() -> TestResult {
    let mut p = Project::new();
    let k = BlockKind::Component;
    let a = p.add_block(None, &spec("A", k))?;
    let b = p.add_block(None, &spec("B", k))?;
    p.move_block(b, Cell::new(2, 0))?;
    // Column 1 exists because of C below; the cell between A and B stays empty.
    let c = p.add_block(None, &spec("C", k))?;
    p.move_block(c, Cell::new(1, 1))?;
    let rel = p.connect_existing(a, Side::Right, b, Direction::Out, "calls", Placement::Keep)?;
    let line = |p: &Project| -> Result<Vec<Pos>, Box<dyn std::error::Error>> {
        let l = render(p, None);
        Ok(l.lines
            .iter()
            .find(|g| g.relation == rel)
            .ok_or("line")?
            .points
            .clone())
    };
    let points = line(&p)?;
    assert_eq!(points.len(), 2, "{points:?}");
    assert!((points[0].y - points[1].y).abs() < 0.01);
    // A box in between: the line goes around it.
    let mid = p.add_block(None, &spec("Mid", k))?;
    p.move_block(mid, Cell::new(1, 0))?;
    let around = line(&p)?;
    assert!(around.len() > 2, "{around:?}");
    let r = render(&p, None).block(mid).ok_or("mid")?.rect;
    for w in around.windows(2) {
        let crosses_mid = w[0].y > r.min.y
            && w[0].y < r.max.y
            && w[0].x.min(w[1].x) < r.max.x
            && w[0].x.max(w[1].x) > r.min.x;
        assert!(!crosses_mid || (w[0].y - w[1].y).abs() > 0.01);
    }
    Ok(())
}

#[test]
fn a_line_from_the_frame_runs_straight_across_empty_cells() -> TestResult {
    let mut p = Project::new();
    let user = p.add_block(None, &spec("User", BlockKind::Person))?;
    let (shop, rel) = p.connect_new(
        user,
        Side::Right,
        &spec("Shop", BlockKind::Component),
        Direction::Out,
        "uses",
    )?;
    let left = p.add_block(Some(shop), &spec("Left", BlockKind::Component))?;
    let inner = p.add_block(Some(shop), &spec("Inner", BlockKind::Component))?;
    p.move_block(left, Cell::new(0, 1))?;
    p.move_block(inner, Cell::new(2, 0))?;
    p.attach(rel, End::B, shop, inner, Side::Left)?;
    let l = render(&p, Some(shop));
    let points = &l
        .lines
        .iter()
        .find(|g| g.relation == rel)
        .ok_or("line")?
        .points;
    assert_eq!(points.len(), 2, "{points:?}");
    Ok(())
}

#[test]
fn relations_between_the_same_boxes_are_drawn_as_one_line() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &spec("A", BlockKind::Component))?;
    let (b, provide) = p.connect_new(
        a,
        Side::Right,
        &spec("B", BlockKind::Component),
        Direction::Out,
        "provide",
    )?;
    let require =
        p.connect_existing(b, Side::Left, a, Direction::Out, "require", Placement::Keep)?;
    let l = render(&p, None);
    assert_eq!(l.lines.len(), 1);
    let line = &l.lines[0];
    assert_eq!(line.relation, provide);
    assert_eq!(line.bundle, vec![require]);
    assert_eq!(line.text, "provide, require");
    // A sends one way, B the other: arrows at both ends.
    assert!(line.arrow_start && line.arrow_end);
    // Switched off, every relation is its own line again.
    p.set_bundle(false);
    assert_eq!(render(&p, None).lines.len(), 2);
    Ok(())
}

#[test]
fn frame_ends_name_the_partner_down_to_this_depth() -> TestResult {
    // A and B inside the system S; the relation A -> B lands on a1 and on b1, b2.
    let mut p = Project::new();
    let s = p.add_block(None, &spec("S", BlockKind::Component))?;
    let a = p.add_block(Some(s), &spec("A", BlockKind::Component))?;
    let (b, rel) = p.connect_new(
        a,
        Side::Right,
        &spec("B", BlockKind::Component),
        Direction::Bi,
        "calls",
    )?;
    let a1 = p.add_block(Some(a), &spec("a1", BlockKind::Component))?;
    let b1 = p.add_block(Some(b), &spec("b1", BlockKind::Component))?;
    let b2 = p.add_block(Some(b), &spec("b2", BlockKind::Component))?;
    p.attach(rel, End::A, a, a1, Side::Right)?;
    p.attach(rel, End::B, b, b1, Side::Left)?;
    p.attach(rel, End::B, b, b2, Side::Left)?;
    let partner = |p: &Project, d| -> Result<String, Box<dyn std::error::Error>> {
        let l = render(p, Some(d));
        Ok(l.lines
            .iter()
            .find_map(|g| g.frame_port.as_ref().map(|f| f.2.clone()))
            .ok_or("no frame end")?)
    };
    assert_eq!(partner(&p, a)?, "B \u{203a} b1, b2");
    assert_eq!(partner(&p, b)?, "A \u{203a} a1");
    // Longer than the label limit: shortened, the full name in the legend.
    p.set_label_limit(Some(8));
    let l = render(&p, Some(a));
    let shown = l
        .lines
        .iter()
        .find_map(|g| g.frame_port.as_ref().map(|f| f.2.clone()))
        .ok_or("no frame end")?;
    assert_eq!(shown, "B \u{203a} [1]");
    assert!(
        l.legend
            .iter()
            .any(|e| e.key == "[1]" && e.lines.join(" ").contains("b1, b2"))
    );
    Ok(())
}

#[test]
fn partner_names_on_top_and_bottom_of_a_frame_do_not_overlap() -> TestResult {
    let mut p = Project::new();
    let shop = p.add_block(None, &spec("Shop", BlockKind::Component))?;
    for name in [
        "Payment Service Provider",
        "Warehouse Management",
        "Customer Relationship",
    ] {
        p.connect_new(
            shop,
            Side::Bottom,
            &spec(name, BlockKind::ExternalSystem),
            Direction::Out,
            "",
        )?;
    }
    for name in ["A", "B", "C", "D"] {
        p.add_block(Some(shop), &spec(name, BlockKind::Component))?;
    }
    let l = render(&p, Some(shop));
    // The ends keep the usual port spacing; the names give way: wrapped, or on the
    // other side of their end.
    let mut ends: Vec<f32> = l
        .lines
        .iter()
        .filter_map(|g| g.frame_port.as_ref())
        .map(|(port, _, _)| port.x)
        .collect();
    ends.sort_by(f32::total_cmp);
    assert_eq!(ends.len(), 3);
    for w in ends.windows(2) {
        assert!((w[1] - w[0] - PORT_SPACING).abs() < 0.01, "{ends:?}");
    }
    let names = scene::frame_label_rects(&l);
    assert_eq!(names.len(), 3);
    for (i, (_, a)) in names.iter().enumerate() {
        for (_, b) in &names[i + 1..] {
            assert!(!a.overlaps(b), "{a:?} overlaps {b:?}");
        }
    }
    Ok(())
}

#[test]
fn a_lone_line_leaves_the_middle_of_its_side() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &spec("A", BlockKind::Component))?;
    let (b, ab) = p.connect_new(
        a,
        Side::Right,
        &spec("B", BlockKind::Component),
        Direction::Out,
        "",
    )?;
    let l = render(&p, None);
    let pts = line_of(&l, ab)?;
    let (ra, rb) = (l.block(a).ok_or("a")?.rect, l.block(b).ok_or("b")?.rect);
    assert_eq!(pts.len(), 2);
    assert!((pts[0].y - ra.center().y).abs() < 0.01, "{pts:?}");
    assert!((pts[1].y - rb.center().y).abs() < 0.01, "{pts:?}");
    Ok(())
}

#[test]
fn a_diagonal_partner_gets_one_bend_from_the_middle_of_each_side() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &spec("A", BlockKind::Component))?;
    let b = p.add_block(None, &spec("B", BlockKind::Component))?;
    p.move_block(b, Cell::new(2, 1))?;
    let ab = p.connect_existing(a, Side::Right, b, Direction::Out, "", Placement::Keep)?;
    let l = render(&p, None);
    let pts = line_of(&l, ab)?;
    let (ra, rb) = (l.block(a).ok_or("a")?.rect, l.block(b).ok_or("b")?.rect);
    // Out of A's right side at its middle, one corner, into B's top at its middle.
    assert_eq!(pts.len(), 3, "{pts:?}");
    assert!((pts[0].y - ra.center().y).abs() < 0.01, "{pts:?}");
    assert!((pts[1].x - rb.center().x).abs() < 0.01 && (pts[1].y - ra.center().y).abs() < 0.01);
    assert!((pts[2].x - rb.center().x).abs() < 0.01 && (pts[2].y - rb.min.y).abs() < 0.01);
    Ok(())
}

#[test]
fn a_bend_that_would_cross_lines_takes_the_gaps_instead() -> TestResult {
    let mut p = Project::new();
    let k = BlockKind::Component;
    let a = p.add_block(None, &spec("A", k))?;
    let b = p.add_block(None, &spec("B", k))?;
    p.move_block(b, Cell::new(3, 2))?;
    // Two straight lines down through the cells the bend's first leg would use.
    for col in [1, 2] {
        let c = p.add_block(None, &spec(&format!("C{col}"), k))?;
        p.move_block(c, Cell::new(col, -1))?;
        let d = p.add_block(None, &spec(&format!("D{col}"), k))?;
        p.move_block(d, Cell::new(col, 1))?;
        p.connect_existing(c, Side::Bottom, d, Direction::Out, "", Placement::Keep)?;
    }
    let ab = p.connect_existing(a, Side::Right, b, Direction::Out, "", Placement::Keep)?;
    let l = render(&p, None);
    assert!(line_of(&l, ab)?.len() > 3, "{:?}", line_of(&l, ab)?);
    assert_eq!(layout::crossings(&l), Vec::new());
    Ok(())
}

#[test]
fn lines_sharing_a_gap_run_in_lanes_and_enter_their_box_in_order() -> TestResult {
    // Three boxes in one column, each with a line into the top of a box below left:
    // the lines run down the same gap and along the same gap, and the ports take the
    // order the lanes arrive in, so nothing crosses.
    let mut p = Project::new();
    let k = BlockKind::Component;
    let t = p.add_block(None, &spec("T", k))?;
    p.move_block(t, Cell::new(0, 3))?;
    // Filled cells in between, so no line can take one bend through them.
    for (col, row) in [
        (1, 0),
        (2, 0),
        (1, 1),
        (2, 1),
        (1, 2),
        (2, 2),
        (0, 1),
        (0, 2),
    ] {
        let f = p.add_block(None, &spec(&format!("F{col}{row}"), k))?;
        p.move_block(f, Cell::new(col, row))?;
    }
    let mut rels = Vec::new();
    let mut x0 = None;
    for row in 0..3 {
        let x = p.add_block(None, &spec(&format!("X{row}"), k))?;
        p.move_block(x, Cell::new(3, row))?;
        x0.get_or_insert(x);
        let rel = p.connect_existing(x, Side::Left, t, Direction::Out, "", Placement::Keep)?;
        p.set_side_at(rel, t, Side::Top)?;
        rels.push(rel);
    }
    let l = render(&p, None);
    assert_eq!(layout::crossings(&l), Vec::new());
    let rt = l.block(t).ok_or("t")?.rect;
    let rx = l.block(x0.ok_or("x0")?).ok_or("x0")?.rect;
    // Down the gap between the columns: three lanes, LANE apart, in a gap wide
    // enough for them.
    let mut xs: Vec<f32> = rels
        .iter()
        .map(|rel| {
            let pts = line_of(&l, *rel)?;
            pts.windows(2)
                .find(|w| (w[0].x - w[1].x).abs() < 0.01 && w[0].x > rt.max.x && w[0].x < rx.min.x)
                .map(|w| w[0].x)
                .ok_or_else(|| format!("{rel:?} has no run down the gap: {pts:?}").into())
        })
        .collect::<Result<_, Box<dyn std::error::Error>>>()?;
    xs.sort_by(f32::total_cmp);
    for w in xs.windows(2) {
        assert!((w[1] - w[0] - layout::LANE).abs() < 0.01, "{xs:?}");
    }
    assert!(rx.min.x - rt.max.x >= 4.0 * layout::LANE - 0.01);
    // Every line enters T through its top, at its own port.
    let mut ports: Vec<f32> = rels
        .iter()
        .map(|rel| line_of(&l, *rel).map(|pts| pts[pts.len() - 1].x))
        .collect::<Result<_, _>>()?;
    ports.sort_by(f32::total_cmp);
    for w in ports.windows(2) {
        assert!(w[1] - w[0] >= PORT_SPACING - 0.01, "{ports:?}");
    }
    Ok(())
}

#[test]
fn a_relation_enters_a_whitebox_next_to_the_box_that_takes_it() -> TestResult {
    let mut p = Project::new();
    let user = p.add_block(None, &spec("User", BlockKind::Person))?;
    let (shop, rel) = p.connect_new(
        user,
        Side::Right,
        &spec("Shop", BlockKind::Component),
        Direction::Out,
        "",
    )?;
    // Inside: Web on the left, Db on the right; the user's line lands on Db.
    let web = p.add_block(Some(shop), &spec("Web", BlockKind::Component))?;
    let db = p.add_block(Some(shop), &spec("Db", BlockKind::Database))?;
    p.move_block(web, Cell::new(0, 0))?;
    p.move_block(db, Cell::new(1, 0))?;
    let view = diagram_view(&p, Some(shop));
    let frame_side = |v: &whiteboxed::view::DiagramView| {
        v.lines.iter().find_map(|l| match (&l.a, &l.b) {
            (ViewEnd::Frame { side, .. }, _) | (_, ViewEnd::Frame { side, .. }) => Some(*side),
            _ => None,
        })
    };
    // Not landed yet: the side it has one level up (Shop's left).
    assert_eq!(frame_side(&view), Some(Side::Left));
    p.attach(rel, End::B, shop, db, Side::Left)?;
    let view = diagram_view(&p, Some(shop));
    assert_eq!(frame_side(&view), Some(Side::Right));
    // Db faces that side too, and the line runs straight in without crossing Web.
    assert!(view.lines.iter().any(
        |l| matches!(l.b, ViewEnd::Block { block, side: Side::Right } if block == db)
            || matches!(l.a, ViewEnd::Block { block, side: Side::Right } if block == db)
    ));
    let l = render(&p, Some(shop));
    let line = l.lines.iter().find(|g| g.relation == rel).ok_or("line")?;
    assert_eq!(line.points.len(), 2, "{:?}", line.points);
    Ok(())
}

#[test]
fn a_frame_side_chosen_by_hand_stays_until_set_back() -> TestResult {
    let mut p = Project::new();
    let user = p.add_block(None, &spec("User", BlockKind::Person))?;
    let (shop, rel) = p.connect_new(
        user,
        Side::Right,
        &spec("Shop", BlockKind::Component),
        Direction::Out,
        "",
    )?;
    let web = p.add_block(Some(shop), &spec("Web", BlockKind::Component))?;
    p.attach(rel, End::B, shop, web, Side::Left)?;
    let side = |p: &Project| {
        diagram_view(p, Some(shop))
            .lines
            .iter()
            .find_map(|l| match (&l.a, &l.b) {
                (ViewEnd::Frame { side, .. }, _) | (_, ViewEnd::Frame { side, .. }) => Some(*side),
                _ => None,
            })
    };
    p.set_frame_side(rel, shop, Some(Side::Bottom))?;
    assert_eq!(side(&p), Some(Side::Bottom));
    p.move_block(web, Cell::new(3, 0))?;
    assert_eq!(side(&p), Some(Side::Bottom), "a move keeps it");
    let back = whiteboxed::persist::from_yaml(&whiteboxed::persist::to_yaml(&p)?)?;
    assert_eq!(side(&back), Some(Side::Bottom));
    p.set_frame_side(rel, shop, None)?;
    assert_eq!(side(&p), Some(Side::Left));
    let other = p.add_block(None, &spec("Other", BlockKind::Component))?;
    assert_eq!(
        p.set_frame_side(rel, other, Some(Side::Top)),
        Err(whiteboxed::model::ModelError::NotOnChain)
    );
    Ok(())
}

/// A and B in one row with `between` boxes in between, and A's line to B.
fn far_apart(
    between: usize,
) -> Result<(Project, BlockId, BlockId, whiteboxed::model::RelationId), Box<dyn std::error::Error>>
{
    let mut p = Project::new();
    let k = BlockKind::Component;
    let a = p.add_block(None, &spec("A", k))?;
    let b = p.add_block(None, &spec("B", k))?;
    p.move_block(b, Cell::new(between as i32 + 1, 0))?;
    for col in 1..=between {
        let f = p.add_block(None, &spec(&format!("F{col}"), k))?;
        p.move_block(f, Cell::new(col as i32, 0))?;
    }
    let ab = p.connect_existing(a, Side::Right, b, Direction::Out, "orders", Placement::Keep)?;
    Ok((p, a, b, ab))
}

#[test]
fn a_line_over_more_than_two_boxes_goes_through_mouseholes() -> TestResult {
    use whiteboxed::model::MouseholeMode;
    let (mut p, a, b, ab) = far_apart(3)?;
    p.set_relation_short(ab, "ord")?;
    let l = render(&p, None);
    let line = l.lines.iter().find(|g| g.relation == ab).ok_or("line")?;
    let m = line.mousehole.as_ref().ok_or("no mouseholes")?;
    let (ra, rb) = (l.block(a).ok_or("a")?.rect, l.block(b).ok_or("b")?.rect);
    // A short piece out of each box, then the hole; the route in between stays known.
    assert!((m.piece_a[0].x - ra.max.x).abs() < 0.01, "{:?}", m.piece_a);
    assert!((m.piece_a[0].dist(m.a) - layout::HOLE_IN).abs() < 0.01);
    assert!(
        (m.piece_b.last().ok_or("b end")?.x - rb.min.x).abs() < 0.01,
        "{:?}",
        m.piece_b
    );
    assert!(line.points.len() > 2);
    // The holes carry the legend number, and the legend the text.
    assert_eq!(m.label, "1");
    assert_eq!(l.legend.len(), 1);
    assert_eq!(
        (l.legend[0].key.as_str(), l.legend[0].lines.join(" ")),
        ("[1]", "orders".to_owned())
    );
    // The pieces are what is drawn and hit: the hole counts, the hidden route does not.
    let middle = line.points[line.points.len() / 2];
    assert!(
        matches!(whiteboxed::hit::hit(&p, &l, m.a, 1.0), whiteboxed::hit::Hit::Line(r, _) if r == ab)
    );
    assert!(
        matches!(
            whiteboxed::hit::hit(&p, &l, middle, 1.0),
            whiteboxed::hit::Hit::Empty
        ),
        "{middle:?}"
    );
    // The holes show the number and the short label; the text is in the legend.
    let svg = whiteboxed::export::to_svg(&scene::scene(&l));
    assert_eq!(svg.matches(">1 ord</text>").count(), 2, "{svg}");
    // What those labels cover starts just past each hole.
    let covered = scene::hole_label_rects(line);
    assert_eq!(covered.len(), 2);
    assert!(
        (covered[0].min.x - (m.a.x + scene::HOLE_R + 5.0)).abs() < 0.01,
        "{covered:?}"
    );
    assert!(
        covered[1].max.x <= m.b.x - scene::HOLE_R - 5.0 + 0.01,
        "{covered:?}"
    );
    // Two boxes in between are fine; told to, even a short line goes under.
    let (p2, _, _, ab2) = far_apart(2)?;
    let l2 = render(&p2, None);
    assert!(
        l2.lines
            .iter()
            .find(|g| g.relation == ab2)
            .ok_or("line")?
            .mousehole
            .is_none()
    );
    p.set_mousehole(ab, None, MouseholeMode::Never)?;
    let l3 = render(&p, None);
    let drawn = l3.lines.iter().find(|g| g.relation == ab).ok_or("line")?;
    assert!(drawn.mousehole.is_none());
    // No number any more; only the short label explains itself in the legend.
    assert!(
        !l3.legend.iter().any(|e| e.key.starts_with('[')),
        "{:?}",
        l3.legend
    );
    let mut short = Project::new();
    let sa = short.add_block(None, &spec("A", BlockKind::Component))?;
    let (_, sab) = short.connect_new(
        sa,
        Side::Right,
        &spec("B", BlockKind::Component),
        Direction::Out,
        "",
    )?;
    short.set_mousehole(sab, None, MouseholeMode::Always)?;
    let l4 = render(&short, None);
    let g = l4.lines.iter().find(|g| g.relation == sab).ok_or("line")?;
    let m4 = g.mousehole.as_ref().ok_or("no mouseholes")?;
    assert!(m4.a.dist(g.points[0]) <= g.points[0].dist(g.points[1]) / 3.0 + 0.01);
    // Without a text the legend names the two ends.
    assert_eq!(l4.legend[0].lines.join(" "), "A \u{2192} B");
    Ok(())
}

#[test]
fn a_line_through_mouseholes_crosses_nothing_on_its_way() -> TestResult {
    let (mut p, _, _, ab) = far_apart(3)?;
    // A straight line down into the middle box, across the hidden route.
    let f2 = p
        .blocks_in(None)
        .find(|(_, b)| b.name == "F2")
        .ok_or("f2")?
        .0;
    let c = p.add_block(None, &spec("C", BlockKind::Component))?;
    p.move_block(c, Cell::new(2, -2))?;
    p.connect_existing(c, Side::Bottom, f2, Direction::Out, "", Placement::Keep)?;
    let l = render(&p, None);
    assert!(
        l.lines
            .iter()
            .find(|g| g.relation == ab)
            .ok_or("line")?
            .mousehole
            .is_some()
    );
    assert_eq!(layout::crossings(&l), Vec::new());
    Ok(())
}

/// A whitebox, the boxes inside it and the relation entering it.
type FannedIn = (
    Project,
    BlockId,
    Vec<BlockId>,
    whiteboxed::model::RelationId,
);

/// A whitebox `A` with `names` boxes in one row, and a sibling below `A` whose
/// relation into `A` is attached to the boxes at `attached` (indices).
fn fanned_in(
    names: &[&str],
    attached: &[usize],
    text: &str,
) -> Result<FannedIn, Box<dyn std::error::Error>> {
    let mut p = Project::new();
    let a = p.add_block(None, &spec("A", BlockKind::Component))?;
    let (_, rel) = p.connect_new(
        a,
        Side::Bottom,
        &spec("X", BlockKind::Component),
        Direction::In,
        text,
    )?;
    let mut inner = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let b = p.add_block(Some(a), &spec(name, BlockKind::Component))?;
        p.move_block(b, Cell::new(i as i32, 0))?;
        inner.push(b);
    }
    for i in attached {
        p.attach(rel, End::A, a, inner[*i], Side::Bottom)?;
    }
    Ok((p, a, inner, rel))
}

#[test]
fn lines_of_different_relations_never_run_along_each_other() -> TestResult {
    // Three lines meet right of `App`: one from `Api` down into `Hooks`, one from
    // `App` into `Hooks`, one from `App` up into `Auth`. The first and the third
    // used to share a stretch at the height of their ports.
    let mut p = Project::new();
    let k = BlockKind::Component;
    let api = p.add_block(None, &spec("Api", k))?;
    let app = p.add_block(None, &spec("App", k))?;
    let hooks = p.add_block(None, &spec("Hooks", k))?;
    let auth = p.add_block(None, &spec("Auth", k))?;
    for (b, col, row) in [(api, 0, 0), (app, 0, 1), (hooks, 1, 1), (auth, 1, 0)] {
        p.move_block(b, Cell::new(col, row))?;
    }
    p.connect_existing(
        api,
        Side::Right,
        hooks,
        Direction::Out,
        "hooks",
        Placement::Keep,
    )?;
    p.connect_existing(
        app,
        Side::Right,
        hooks,
        Direction::Out,
        "events",
        Placement::Keep,
    )?;
    p.connect_existing(
        app,
        Side::Right,
        auth,
        Direction::Out,
        "ports",
        Placement::Keep,
    )?;
    let l = render(&p, None);
    assert_eq!(layout::overlaps(&l), Vec::new());
    // The two lines are an X: at most one clean crossing instead of merging.
    assert!(
        layout::crossings(&l).len() <= 1,
        "{:?}",
        layout::crossings(&l)
    );
    Ok(())
}

#[test]
fn lines_running_along_each_other_are_reported() -> TestResult {
    let (p, _) = sample()?;
    let mut l = render(&p, None);
    assert_eq!(layout::overlaps(&l), Vec::new());
    // Two relations put on one horizontal: they share the stretch from 50 to 100.
    l.lines[0].points = vec![Pos::new(0.0, 10.0), Pos::new(100.0, 10.0)];
    l.lines[1].points = vec![Pos::new(150.0, 10.0), Pos::new(50.0, 10.0)];
    assert_ne!(l.lines[0].relation, l.lines[1].relation);
    let found = layout::overlaps(&l);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!((found[0].a, found[0].b), (0, 1));
    assert_eq!(
        (found[0].from, found[0].to),
        (Pos::new(50.0, 10.0), Pos::new(100.0, 10.0))
    );
    // Touching at an end is no overlap; a vertical one is found as well.
    l.lines[1].points = vec![Pos::new(100.0, 10.0), Pos::new(150.0, 10.0)];
    assert_eq!(layout::overlaps(&l), Vec::new());
    l.lines[0].points = vec![Pos::new(10.0, 0.0), Pos::new(10.0, 100.0)];
    l.lines[1].points = vec![Pos::new(10.0, 80.0), Pos::new(10.0, 300.0)];
    assert_eq!(layout::overlaps(&l).len(), 1);
    // The score counts it with the crossings.
    let s = whiteboxed::score::score(&p, &l);
    assert_eq!((s.crossings, s.overlaps), (0, 1));
    assert!(s.crossings_level > whiteboxed::score::Level::Green);
    assert!(
        s.causes()
            .iter()
            .any(|c| c.what == "1 line running along another"),
        "{:?}",
        s.causes()
    );
    Ok(())
}

#[test]
fn a_fanned_out_relation_names_its_partner_once() -> TestResult {
    let (p, a, inner, rel) = fanned_in(&["b1", "b2", "b3", "b4", "b5"], &[0, 1, 2, 3, 4], "uses")?;
    let l = render(&p, Some(a));
    let branches: Vec<&layout::LineGeom> = l.lines.iter().filter(|g| g.relation == rel).collect();
    assert_eq!(branches.len(), inner.len());
    assert_eq!(branches.iter().filter(|g| g.frame_named).count(), 1);
    assert_eq!(branches.iter().filter(|g| !g.text.is_empty()).count(), 1);
    let names = scene::frame_label_rects(&l);
    assert_eq!(
        names
            .iter()
            .filter(|(li, _)| l.lines[*li].relation == rel)
            .count(),
        1,
        "{names:?}"
    );
    Ok(())
}

#[test]
fn branches_through_mouseholes_get_their_own_numbers() -> TestResult {
    use whiteboxed::model::MouseholeMode;
    let (mut p, a, inner, rel) = fanned_in(&["b1", "b2", "b3"], &[0, 1, 2], "uses")?;
    p.set_mousehole(rel, Some(a), MouseholeMode::Always)?;
    let l = render(&p, Some(a));
    let holes: Vec<&layout::Mousehole> = l
        .lines
        .iter()
        .filter(|g| g.relation == rel)
        .filter_map(|g| g.mousehole.as_ref())
        .collect();
    assert_eq!(holes.len(), inner.len());
    // Every branch has its own pair of holes and its own frame point next to the
    // others; multiplied like that, the relation is one legend number and each
    // hole says which of the three it is.
    let labels: Vec<&str> = holes.iter().map(|m| m.label.as_str()).collect();
    assert_eq!(labels, vec!["1 1/3", "1 2/3", "1 3/3"]);
    let frame_ports: Vec<Pos> = l
        .lines
        .iter()
        .filter(|g| g.relation == rel)
        .filter_map(|g| g.frame_port.as_ref().map(|(p, _, _)| *p))
        .collect();
    for (i, p) in frame_ports.iter().enumerate() {
        for q in &frame_ports[i + 1..] {
            assert!(p.dist(*q) >= PORT_SPACING - 0.01, "{frame_ports:?}");
        }
    }
    let frame_holes: Vec<Pos> = holes.iter().map(|m| m.b).collect();
    for (i, p) in frame_holes.iter().enumerate() {
        for q in &frame_holes[i + 1..] {
            assert!(p.dist(*q) >= PORT_SPACING - 0.01, "{frame_holes:?}");
        }
    }
    // The partner is still named once, at the branch in the middle.
    let named: Vec<Option<BlockId>> = l
        .lines
        .iter()
        .filter(|g| g.relation == rel && g.frame_named)
        .map(|g| g.landing)
        .collect();
    assert_eq!(named, vec![Some(inner[0]), Some(inner[1]), Some(inner[2])]);
    let tags: Vec<String> = l
        .lines
        .iter()
        .filter(|g| g.relation == rel)
        .filter_map(|g| g.frame_port.as_ref().map(|(_, _, name)| name.clone()))
        .collect();
    assert_eq!(tags, vec!["[1] 1/3", "[1] 2/3", "[1] 3/3"]);
    let legend: Vec<(String, String)> = l
        .legend
        .iter()
        .map(|e| (e.key.clone(), e.lines.join(" ")))
        .collect();
    assert_eq!(
        legend,
        vec![("[1]".to_owned(), "uses \u{2014} X".to_owned())]
    );
    // Each tag is written at both holes of its branch, and the branches carry no
    // text of their own.
    let svg = export::to_svg(&scene::scene(&l));
    for k in 1..=3 {
        assert_eq!(svg.matches(&format!(">1 {k}/3</text>")).count(), 2, "{svg}");
    }
    assert!(
        l.lines
            .iter()
            .filter(|g| g.relation == rel)
            .all(|g| g.text.is_empty())
    );
    Ok(())
}

#[test]
fn an_entering_relation_takes_the_frame_point_under_its_box() -> TestResult {
    // One relation fans out to all six boxes from the middle of the bottom edge;
    // a second one, attached to the leftmost box only, enters right under it.
    let (mut p, a, inner, _) = fanned_in(
        &["b1", "b2", "b3", "b4", "b5", "b6"],
        &[0, 1, 2, 3, 4, 5],
        "use cases",
    )?;
    let (_, snapshot) = p.connect_new(
        a,
        Side::Bottom,
        &spec("Y", BlockKind::Component),
        Direction::In,
        "snapshot",
    )?;
    p.attach(snapshot, End::A, a, inner[0], Side::Bottom)?;
    let l = render(&p, Some(a));
    let line = l
        .lines
        .iter()
        .find(|g| g.relation == snapshot)
        .ok_or("line")?;
    let (port, side, _) = line.frame_port.as_ref().ok_or("frame port")?;
    let b1 = l.block(inner[0]).ok_or("b1")?.rect;
    assert_eq!(*side, Side::Bottom);
    assert!(
        port.x >= b1.min.x && port.x <= b1.max.x,
        "frame point at {port:?}, b1 spans {b1:?}"
    );
    assert_eq!(line.points.len(), 2, "straight: {:?}", line.points);
    assert_eq!(layout::crossings(&l), Vec::new());
    assert!(line.mousehole.is_none());
    Ok(())
}

#[test]
fn a_label_keeps_off_boxes() -> TestResult {
    // The line from the lower left box comes up the gap and turns into the box on
    // the right over a stretch shorter than its text.
    let mut p = Project::new();
    let k = BlockKind::Component;
    let a = p.add_block(None, &spec("A", k))?;
    let b = p.add_block(None, &spec("B", k))?;
    p.move_block(a, Cell::new(0, 1))?;
    p.move_block(b, Cell::new(1, 0))?;
    let rel = p.connect_existing(
        a,
        Side::Right,
        b,
        Direction::Out,
        "catalog queries",
        Placement::Keep,
    )?;
    p.set_side_at(rel, b, Side::Left)?;
    let l = render(&p, None);
    let line = l.lines.iter().find(|g| g.relation == rel).ok_or("line")?;
    assert_eq!(line.text, "catalog queries");
    assert!(!line.label_horizontal, "beside the long vertical: {line:?}");
    for (li, rect) in scene::label_rects(&l) {
        for block in &l.blocks {
            assert!(
                !block.rect.overlaps(&rect),
                "label of {} covers {}",
                l.lines[li].text,
                block.name
            );
        }
    }
    Ok(())
}

#[test]
fn a_label_moves_to_the_other_side_of_its_line_when_another_runs_through_it() -> TestResult {
    // Two lines leave the top of S next to each other: the one to O turns right
    // through where the label of the one to U used to be written, so that label
    // goes left of its line.
    let mut p = Project::new();
    let s = p.add_block(None, &spec("S", BlockKind::Component))?;
    let (_, dashboard) = p.connect_new(
        s,
        Side::Top,
        &spec("U", BlockKind::Person),
        Direction::In,
        "dashboard",
    )?;
    let (o, auth) = p.connect_new(
        s,
        Side::Top,
        &spec("O", BlockKind::ExternalSystem),
        Direction::Out,
        "auth code",
    )?;
    p.move_block(o, Cell::new(1, -1))?;
    p.set_side_at(auth, s, Side::Top)?;
    let l = render(&p, None);
    let labels = scene::label_rects(&l);
    for (li, rect) in &labels {
        for (lj, other) in l.lines.iter().enumerate() {
            if other.relation == l.lines[*li].relation {
                continue;
            }
            assert!(
                !other
                    .points
                    .windows(2)
                    .any(|w| layout::segment_meets(w[0], w[1], rect)),
                "line {lj} runs through the label of {}",
                l.lines[*li].text
            );
        }
    }
    let texts: Vec<(whiteboxed::model::RelationId, &str)> = l
        .lines
        .iter()
        .map(|g| (g.relation, g.text.as_str()))
        .collect();
    assert!(texts.contains(&(auth, "auth code")), "{texts:?}");
    assert!(texts.contains(&(dashboard, "dashboard")), "{texts:?}");
    let line = l
        .lines
        .iter()
        .find(|g| g.relation == dashboard)
        .ok_or("line")?;
    assert!(!line.label_horizontal && line.label_flipped, "{line:?}");
    assert!(l.legend.is_empty());
    Ok(())
}

#[test]
fn a_label_slides_along_its_line_to_a_clear_spot() -> TestResult {
    // Two lines run down one gap side by side past the middle row: beside the
    // first, at mid-height, is the other line or a box, and its short stems hug
    // its boxes. Further up, between the rows, there is room left of it.
    let mut p = Project::new();
    let k = BlockKind::Component;
    let mut at = |name: &str, col: i32, row: i32| -> Result<BlockId, Box<dyn std::error::Error>> {
        let id = p.add_block(None, &spec(name, k))?;
        p.move_block(id, Cell::new(col, row))?;
        Ok(id)
    };
    let a = at("A", 0, 0)?;
    let e = at("E", 0, 1)?;
    let b = at("B", 0, 2)?;
    let c = at("C", 1, 0)?;
    let f = at("F", 1, 1)?;
    let d = at("D", 1, 2)?;
    let ad = p.connect_existing(
        a,
        Side::Right,
        d,
        Direction::Out,
        "dashboard",
        Placement::Keep,
    )?;
    p.set_side_at(ad, d, Side::Left)?;
    let cb = p.connect_existing(
        c,
        Side::Left,
        b,
        Direction::Out,
        "auth code",
        Placement::Keep,
    )?;
    p.set_side_at(cb, b, Side::Right)?;
    let l = render(&p, None);
    let texts: Vec<(whiteboxed::model::RelationId, &str)> = l
        .lines
        .iter()
        .map(|g| (g.relation, g.text.as_str()))
        .collect();
    assert_eq!(
        texts,
        vec![(ad, "dashboard"), (cb, "auth code")],
        "{:?}",
        l.lines
    );
    assert!(l.legend.is_empty());
    let _ = (e, f);
    // Every label sits clear of every box and line.
    for (li, rect) in scene::label_rects(&l) {
        assert!(l.blocks.iter().all(|b| !b.rect.overlaps(&rect)), "{li}");
        for other in l
            .lines
            .iter()
            .filter(|g| g.relation != l.lines[li].relation)
        {
            assert!(
                !other
                    .points
                    .windows(2)
                    .any(|w| layout::segment_meets(w[0], w[1], &rect)),
                "{li}: {rect:?} vs {:?}",
                other.points
            );
        }
    }
    Ok(())
}

#[test]
fn a_straight_branch_of_a_fanned_out_relation_has_no_jog() -> TestResult {
    // The frame point sits at the mean of the two branches' wishes, which the wider
    // right box moves a few pixels off the gap the straight branch runs up.
    let mut p = Project::new();
    let a = p.add_block(None, &spec("A", BlockKind::Component))?;
    let (_, rel) = p.connect_new(
        a,
        Side::Bottom,
        &spec("X", BlockKind::Component),
        Direction::In,
        "catalog",
    )?;
    let b1 = p.add_block(Some(a), &spec("b1", BlockKind::Component))?;
    let b2 = p.add_block(
        Some(a),
        &spec("Branches and reports box", BlockKind::Component),
    )?;
    p.move_block(b1, Cell::new(0, 1))?;
    p.move_block(b2, Cell::new(1, 0))?;
    p.attach(rel, End::A, a, b1, Side::Bottom)?;
    p.attach(rel, End::A, a, b2, Side::Bottom)?;
    let l = render(&p, Some(a));
    let (r1, r2) = (l.block(b1).ok_or("b1")?.rect, l.block(b2).ok_or("b2")?.rect);
    let off = (r2.width() - r1.width()) / 4.0;
    assert!(
        off > 2.0 && off < layout::LANE / 2.0,
        "the frame point is {off} px off the gap"
    );
    for g in l.lines.iter().filter(|g| g.relation == rel) {
        let short: Vec<(Pos, Pos)> = g
            .points
            .windows(2)
            .filter(|w| w[0].dist(w[1]) < layout::LANE / 2.0)
            .map(|w| (w[0], w[1]))
            .collect();
        assert!(short.is_empty(), "jog in {:?}: {short:?}", g.points);
    }
    Ok(())
}

#[test]
fn partner_names_at_the_top_keep_off_the_title() -> TestResult {
    // The only box sits at the top left, right under the title; the name of the
    // relation entering above it goes to the next row instead of over the title.
    let mut p = Project::new();
    let a = p.add_block(
        None,
        &spec("A rather long whitebox name indeed", BlockKind::Component),
    )?;
    let (_, rel) = p.connect_new(
        a,
        Side::Top,
        &spec("X", BlockKind::Component),
        Direction::In,
        "",
    )?;
    let b = p.add_block(Some(a), &spec("b", BlockKind::Component))?;
    p.attach(rel, End::A, a, b, Side::Top)?;
    let l = render(&p, Some(a));
    let frame = l.frame.ok_or("frame")?;
    let title = format!("{} (whitebox)", l.title.as_deref().unwrap_or(""));
    let title_rect = Rect::from_min_size(
        Pos::new(
            frame.min.x + 16.0,
            frame.min.y + 20.0 - layout::NAME_SIZE * 0.75,
        ),
        whiteboxed::text::width(&title, layout::NAME_SIZE),
        layout::NAME_SIZE,
    );
    let names = scene::frame_label_rects(&l);
    assert_eq!(names.len(), 1);
    assert!(
        !names[0].1.overlaps(&title_rect),
        "{:?} over the title {title_rect:?}",
        names[0].1
    );
    // The frame point itself starts right of the title as well.
    let line = l.lines.iter().find(|g| g.relation == rel).ok_or("line")?;
    let (port, _, _) = line.frame_port.as_ref().ok_or("frame port")?;
    assert!(
        port.x >= title_rect.max.x,
        "{port:?} under the title {title_rect:?}"
    );
    Ok(())
}

/// `rows` lines from left to right across `cols` lines from top to bottom, all on
/// automatic mouseholes, every line crossing every line of the other direction.
fn grid_of_lines(rows: usize, cols: usize) -> Result<Project, Box<dyn std::error::Error>> {
    let mut p = Project::new();
    let k = BlockKind::Component;
    for r in 0..rows {
        let l = p.add_block(None, &spec(&format!("L{r}"), k))?;
        let rt = p.add_block(None, &spec(&format!("R{r}"), k))?;
        p.move_block(l, Cell::new(0, r as i32))?;
        p.move_block(rt, Cell::new(cols as i32 + 1, r as i32))?;
        p.connect_existing(l, Side::Right, rt, Direction::Out, "", Placement::Keep)?;
    }
    for c in 0..cols {
        let t = p.add_block(None, &spec(&format!("T{c}"), k))?;
        let b = p.add_block(None, &spec(&format!("B{c}"), k))?;
        p.move_block(t, Cell::new(c as i32 + 1, -1))?;
        p.move_block(b, Cell::new(c as i32 + 1, rows as i32))?;
        p.connect_existing(t, Side::Bottom, b, Direction::Out, "", Placement::Keep)?;
    }
    Ok(p)
}

#[test]
fn a_line_crossing_more_than_two_others_goes_under_or_around() -> TestResult {
    // One vertical across three horizontals: it may not cross all three. Going
    // under saves three crossings and costs a hole; a way around saves them for
    // bends alone, so that is taken where it exists.
    let p = grid_of_lines(3, 1)?;
    let l = render(&p, None);
    let under = l.lines.iter().filter(|g| g.mousehole.is_some()).count();
    assert!(
        under == 1 || layout::crossings(&l).len() <= 2,
        "{:?}",
        l.lines
    );
    // Across two horizontals it stays drawn, with two jumps at most.
    let l = render(&grid_of_lines(2, 1)?, None);
    assert!(l.lines.iter().all(|g| g.mousehole.is_none()));
    assert!(layout::crossings(&l).len() <= 2);
    Ok(())
}

#[test]
fn a_grid_of_crossing_lines_ends_with_at_most_two_crossings_per_line() -> TestResult {
    // Three by three: every line crosses three others at first. One hole costs
    // about three crossings, so the picture ends with no line crossing more than
    // two, by holes, detours or both.
    let p = grid_of_lines(3, 3)?;
    let l = render(&p, None);
    let crossings = layout::crossings(&l);
    for (li, g) in l.lines.iter().enumerate() {
        let crossed = crossings
            .iter()
            .filter(|c| c.horizontal == li || c.vertical == li)
            .count();
        assert!(
            crossed <= 2 || g.mousehole.is_some(),
            "line {li} crosses {crossed}"
        );
    }
    assert!(l.lines.iter().filter(|g| g.mousehole.is_some()).count() <= 3);
    // Told never, a line stays drawn however much it crosses.
    let mut p = grid_of_lines(3, 1)?;
    let vertical = p
        .relations
        .iter()
        .find(|(_, r)| {
            p.blocks
                .get(&r.a.anchors[0].block)
                .is_some_and(|b| b.name == "T0")
        })
        .map(|(id, _)| *id)
        .ok_or("vertical")?;
    p.set_mousehole(vertical, None, whiteboxed::model::MouseholeMode::Never)?;
    let l = render(&p, None);
    assert!(l.lines.iter().all(|g| g.mousehole.is_none()));
    assert!(layout::crossings(&l).len() <= 3);
    Ok(())
}

#[test]
fn a_far_branch_gets_its_own_frame_point_before_it_goes_under() -> TestResult {
    // A relation fans out to the first and the last of seven boxes from a frame
    // point in the middle; five more relations come straight down into the boxes
    // between. From the middle, the branch to b1 crosses three of them; from a
    // frame point of its own above b1 it crosses nothing, so it stays drawn.
    let names = ["b1", "b2", "b3", "b4", "b5", "b6", "b7"];
    let (mut p, a, inner, rel) = fanned_in(&names, &[0, 6], "wide")?;
    for (i, b) in inner.iter().enumerate().skip(1).take(5) {
        let (_, r) = p.connect_new(
            a,
            Side::Bottom,
            &spec(&format!("X{i}"), BlockKind::Component),
            Direction::In,
            "",
        )?;
        p.attach(r, End::A, a, *b, Side::Bottom)?;
    }
    let l = render(&p, Some(a));
    assert!(
        l.lines.iter().all(|g| g.mousehole.is_none()),
        "no holes needed"
    );
    let ports: Vec<Pos> = l
        .lines
        .iter()
        .filter(|g| g.relation == rel)
        .filter_map(|g| g.frame_port.as_ref().map(|(p, _, _)| *p))
        .collect();
    assert_eq!(ports.len(), 2);
    assert!(
        ports[0].dist(ports[1]) > PORT_SPACING,
        "fanned out: {ports:?}"
    );
    let (b1, b7) = (
        l.block(inner[0]).ok_or("b1")?.rect,
        l.block(inner[6]).ok_or("b7")?.rect,
    );
    assert!(
        ports.iter().any(|p| p.x >= b1.min.x && p.x <= b1.max.x),
        "{ports:?} {b1:?}"
    );
    assert!(
        ports.iter().any(|p| p.x >= b7.min.x && p.x <= b7.max.x),
        "{ports:?} {b7:?}"
    );
    assert_eq!(layout::crossings(&l), Vec::new());
    // Two frame points make the relation multiplied: each point carries the
    // legend number with its place in the group, the branches no text, the legend
    // the text with the partner.
    let mut tags: Vec<String> = l
        .lines
        .iter()
        .filter(|g| g.relation == rel)
        .filter_map(|g| g.frame_port.as_ref().map(|(_, _, name)| name.clone()))
        .collect();
    tags.sort();
    assert_eq!(tags, vec!["[1] 1/2", "[1] 2/2"]);
    assert!(
        l.lines
            .iter()
            .filter(|g| g.relation == rel)
            .all(|g| g.text.is_empty())
    );
    assert!(
        l.legend
            .iter()
            .any(|e| e.key == "[1]" && e.lines.join(" ") == "wide \u{2014} X"),
        "{:?}",
        l.legend
    );
    Ok(())
}

#[test]
fn a_lane_moves_past_its_neighbours_when_that_undoes_crossings() -> TestResult {
    // Down the gap between A and B run three lines: from T around B into C's top,
    // from A into C's left (which turns off toward C), and from E into F. On the
    // lane nearest A the line from T crosses A's two lines and the one into C
    // twice; on the lane nearest B it crosses only A's line into B.
    let mut p = Project::new();
    let k = BlockKind::Component;
    let mut at = |name: &str, col: i32, row: i32| -> Result<BlockId, Box<dyn std::error::Error>> {
        let id = p.add_block(None, &spec(name, k))?;
        p.move_block(id, Cell::new(col, row))?;
        Ok(id)
    };
    let a = at("A", 0, 0)?;
    let b = at("B", 1, 0)?;
    let c = at("C", 1, 1)?;
    let e = at("E", 0, 1)?;
    let f = at("F", 1, 2)?;
    let t = at("T", 1, -1)?;
    fn at2(
        p: &mut Project,
        name: &str,
        col: i32,
        row: i32,
    ) -> Result<BlockId, Box<dyn std::error::Error>> {
        let id = p.add_block(None, &spec(name, BlockKind::Component))?;
        p.move_block(id, Cell::new(col, row))?;
        Ok(id)
    }
    // The line from T is the oldest relation, so it starts on the lane nearest A.
    let tc = p.connect_existing(t, Side::Bottom, c, Direction::Out, "", Placement::Keep)?;
    p.set_side_at(tc, c, Side::Top)?;
    let ab = p.connect_existing(a, Side::Right, b, Direction::Out, "count", Placement::Keep)?;
    let ef = p.connect_existing(e, Side::Right, f, Direction::Out, "", Placement::Keep)?;
    p.set_side_at(ef, f, Side::Left)?;
    let ac = p.connect_existing(
        a,
        Side::Right,
        c,
        Direction::Out,
        "helpers",
        Placement::Keep,
    )?;
    p.set_side_at(ac, c, Side::Left)?;
    // Lines from B and C to boxes on their right make the gap right of B worse:
    // two crossings there against one left of B, so the line from T goes left.
    let g = at2(&mut p, "G", 2, 0)?;
    let h = at2(&mut p, "H", 2, 1)?;
    p.connect_existing(b, Side::Right, g, Direction::Out, "", Placement::Keep)?;
    p.connect_existing(c, Side::Right, h, Direction::Out, "", Placement::Keep)?;
    let l = render(&p, None);
    let crossings = layout::crossings(&l);
    let pairs: Vec<(u64, u64)> = crossings
        .iter()
        .map(|x| {
            (
                l.lines[x.horizontal].relation.0,
                l.lines[x.vertical].relation.0,
            )
        })
        .collect();
    assert!(pairs.len() <= 1, "{pairs:?}\n{:?}", l.lines);
    assert!(pairs.iter().all(|(_, v)| *v == tc.0), "{pairs:?}");
    let _ = ab;
    Ok(())
}

#[test]
fn a_branch_that_goes_under_keeps_its_frame_point_to_itself() -> TestResult {
    // The first branch goes under (its straight line would cross three boxes), the
    // other two stay drawn. They used to join the hidden branch's frame point, so
    // its hole sat on their stem.
    let mut p = Project::new();
    let a = p.add_block(None, &spec("A", BlockKind::Component))?;
    let (_, rel) = p.connect_new(
        a,
        Side::Top,
        &spec("X", BlockKind::Component),
        Direction::In,
        "uses",
    )?;
    let mut at = |name: &str, col: i32, row: i32| -> Result<BlockId, Box<dyn std::error::Error>> {
        let id = p.add_block(Some(a), &spec(name, BlockKind::Component))?;
        p.move_block(id, Cell::new(col, row))?;
        Ok(id)
    };
    let b1 = at("b1", 1, 3)?;
    for (name, row) in [("c1", 0), ("c2", 1), ("c3", 2)] {
        at(name, 1, row)?;
    }
    let b2 = at("b2", 0, 0)?;
    let b3 = at("b3", 2, 0)?;
    for b in [b1, b2, b3] {
        p.attach(rel, End::A, a, b, Side::Top)?;
    }
    let l = render(&p, Some(a));
    let branches: Vec<&layout::LineGeom> = l.lines.iter().filter(|g| g.relation == rel).collect();
    let under: Vec<Option<BlockId>> = branches
        .iter()
        .filter(|g| g.mousehole.is_some())
        .map(|g| g.landing)
        .collect();
    assert_eq!(under, vec![Some(b1)]);
    let port_of = |b: BlockId| {
        branches
            .iter()
            .find(|g| g.landing == Some(b))
            .and_then(|g| g.frame_port.as_ref().map(|(p, _, _)| *p))
            .ok_or("frame port")
    };
    let (p1, p2, p3) = (port_of(b1)?, port_of(b2)?, port_of(b3)?);
    assert!(p2.dist(p3) < 0.01, "the drawn branches share a frame point");
    assert!(p1.dist(p2) >= PORT_SPACING - 0.01, "{p1:?} vs {p2:?}");
    // No drawn line passes through either hole.
    let hole = branches
        .iter()
        .find_map(|g| g.mousehole.as_ref())
        .ok_or("hole")?;
    for g in l.lines.iter().filter(|g| g.mousehole.is_none()) {
        for h in [hole.a, hole.b] {
            let d = whiteboxed::geom::polyline_dist(h, &g.points);
            assert!(
                d > scene::HOLE_R,
                "line {:?} runs through the hole at {h:?}",
                g.relation
            );
        }
    }
    Ok(())
}

#[test]
fn partner_names_at_the_frame_are_no_labels() -> TestResult {
    // Branches without a text of their own have no label rect at all, and the
    // one that carries the text has the label's rect, not the partner name's.
    let (p, a, _, rel) = fanned_in(&["b1", "b2", "b3"], &[0, 1, 2], "uses")?;
    let l = render(&p, Some(a));
    let frame = l.frame.ok_or("frame")?;
    let labels = scene::label_rects(&l);
    let of_rel: Vec<(usize, Rect)> = labels
        .into_iter()
        .filter(|(li, _)| l.lines[*li].relation == rel)
        .collect();
    assert_eq!(of_rel.len(), 1, "{of_rel:?}");
    let (li, rect) = of_rel[0];
    let line = &l.lines[li];
    assert_eq!(line.text, "uses");
    assert_eq!(
        rect,
        scene::label_rect(
            line.label_at,
            line.label_horizontal,
            line.label_flipped,
            "uses"
        )
    );
    assert!(frame.expand(-8.0).holds(&rect), "{rect:?} inside {frame:?}");
    // The names are still drawn, and listed separately.
    assert_eq!(
        scene::frame_label_rects(&l)
            .iter()
            .filter(|(li, _)| l.lines[*li].relation == rel)
            .count(),
        1
    );
    Ok(())
}

#[test]
fn with_bands_a_relation_never_enters_from_the_bottom_on_its_own() -> TestResult {
    // A sibling below A reaches a box in A's bottom row: without a band it enters
    // from the bottom; with a band across the bottom it takes the next best side.
    let (mut p, a, inner, rel) = fanned_in(&["b1", "b2", "b3"], &[2], "uses")?;
    let side_of = |p: &Project| {
        diagram_view(p, Some(a))
            .lines
            .iter()
            .find_map(|l| match (&l.a, &l.b) {
                (ViewEnd::Frame { side, .. }, _) | (_, ViewEnd::Frame { side, .. }) => Some(*side),
                _ => None,
            })
    };
    assert_eq!(side_of(&p), Some(Side::Bottom));
    p.add_block(Some(a), &spec("Guards", BlockKind::Component).as_band())?;
    assert_ne!(side_of(&p), Some(Side::Bottom));
    let l = render(&p, Some(a));
    let band = l.blocks.iter().find(|b| b.band).ok_or("band")?.rect;
    for g in &l.lines {
        for w in g.points.windows(2) {
            assert!(
                !layout::segment_meets(w[0], w[1], &band),
                "{:?} runs through the band {band:?}",
                g.points
            );
        }
    }
    // Set to bottom by hand, it goes under the band: a hole where it meets the
    // band and one where it leaves it, with its text in the legend, once, even
    // though its short label had an entry there already.
    p.set_relation_short(rel, "u")?;
    p.set_frame_side(rel, a, Some(Side::Bottom))?;
    assert_eq!(side_of(&p), Some(Side::Bottom));
    let l = render(&p, Some(a));
    let band = l.blocks.iter().find(|b| b.band).ok_or("band")?.rect;
    let line = l.lines.iter().find(|g| g.relation == rel).ok_or("line")?;
    let m = line.mousehole.as_ref().ok_or("no hole at the band")?;
    // The arches sit outside the band, their round side toward it.
    let gap = 2.0 * scene::HOLE_R;
    assert!(
        (m.a.y - (band.min.y - gap)).abs() < 0.01,
        "{:?} vs {band:?}",
        m.a
    );
    assert!(
        (m.b.y - (band.max.y + gap)).abs() < 0.01,
        "{:?} vs {band:?}",
        m.b
    );
    assert!(m.piece_a.iter().all(|p| p.y <= band.min.y - gap + 0.01));
    assert!(m.piece_b.iter().all(|p| p.y >= band.max.y + gap - 0.01));
    assert_eq!(m.label, "1");
    let about_uses: Vec<(&str, String)> = l
        .legend
        .iter()
        .filter(|e| e.lines.join(" ") == "uses")
        .map(|e| (e.key.as_str(), e.lines.join(" ")))
        .collect();
    assert_eq!(about_uses, vec![("[1]", "uses".to_owned())]);
    let svg = export::to_svg(&scene::scene(&l));
    assert_eq!(svg.matches(">1 u</text>").count(), 2, "{svg}");
    let _ = inner;
    Ok(())
}

#[test]
fn a_frame_end_pushed_off_its_box_takes_the_box_end_along() -> TestResult {
    // The fanned-out relation's frame point sits at the mean of its three boxes,
    // right over b2; the single relation into b2 wants the same spot, so the two
    // frame points are spread apart. The single line still runs straight: its port
    // on b2 follows its frame point.
    let (mut p, a, inner, _) = fanned_in(&["b1", "b2", "b3"], &[0, 1, 2], "wide")?;
    let (_, single) = p.connect_new(
        a,
        Side::Bottom,
        &spec("Y", BlockKind::Component),
        Direction::In,
        "one",
    )?;
    p.attach(single, End::A, a, inner[1], Side::Bottom)?;
    let l = render(&p, Some(a));
    for g in l.lines.iter().filter(|g| g.relation == single) {
        assert_eq!(g.points.len(), 2, "straight: {:?}", g.points);
    }
    for g in &l.lines {
        let short: Vec<f32> = g
            .points
            .windows(2)
            .map(|w| w[0].dist(w[1]))
            .filter(|d| *d < layout::LANE / 2.0)
            .collect();
        assert!(short.is_empty(), "step in {:?}", g.points);
    }
    Ok(())
}

#[test]
fn a_label_keeps_off_the_pictures_at_line_ends() -> TestResult {
    // A line look with a tall picture at the tail: the label of a short line used to
    // be written over it.
    let mut p = Project::new();
    let k = BlockKind::Component;
    let a = p.add_block(None, &spec("A", k))?;
    let (b, rel) = p.connect_new(a, Side::Right, &spec("B", k), Direction::Out, "snapshot")?;
    use whiteboxed::look::{LineLook, LineLookCopy, LineLookSpec, LookId, Picture};
    let svg = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 40 10'><rect width='40' height='10' fill='currentColor'/></svg>";
    let key = LookId("key".into());
    let spec = LineLookSpec {
        tail: Some(key.clone()),
        ..LineLookSpec::default()
    };
    p.wear_line_look(
        rel,
        Some(LineLookCopy {
            id: LookId("admin".into()),
            look: LineLook::new("admin", spec, 1)?,
            pictures: vec![(key, Picture::new("key", svg, 1)?)],
        }),
    )?;
    let _ = b;
    let l = render(&p, None);
    let line = l.lines.iter().find(|g| g.relation == rel).ok_or("line")?;
    let pictures = scene::end_picture_rects(line);
    assert_eq!(pictures.len(), 1, "{pictures:?}");
    let label = scene::label_rects(&l)
        .into_iter()
        .find(|(li, _)| l.lines[*li].relation == rel)
        .map(|(_, r)| r)
        .ok_or("label")?;
    assert!(
        !label.overlaps(&pictures[0]),
        "label {label:?} over the picture {:?}",
        pictures[0]
    );
    Ok(())
}

#[test]
fn a_straight_line_stays_left_of_a_line_turning_right_on_the_same_side() -> TestResult {
    // On b1's top: a line straight up to its own frame point, pushed right of the
    // whitebox title, and a branch of a fan-out that comes from the right. The
    // straight one keeps the left port, so the branch does not cross it.
    let mut p = Project::new();
    let a = p.add_block(
        None,
        &spec("A very long whitebox name indeed", BlockKind::Component),
    )?;
    let (_, straight) = p.connect_new(
        a,
        Side::Top,
        &spec("X", BlockKind::Component),
        Direction::In,
        "straight",
    )?;
    let (_, fan) = p.connect_new(
        a,
        Side::Top,
        &spec("Y", BlockKind::Component),
        Direction::In,
        "fan",
    )?;
    let mut inner = Vec::new();
    for (i, name) in ["b1", "b2", "b3"].iter().enumerate() {
        let b = p.add_block(Some(a), &spec(name, BlockKind::Component))?;
        p.move_block(b, Cell::new(i as i32, 0))?;
        inner.push(b);
    }
    p.attach(straight, End::A, a, inner[0], Side::Top)?;
    for b in &inner {
        p.attach(fan, End::A, a, *b, Side::Top)?;
    }
    let l = render(&p, Some(a));
    assert_eq!(layout::crossings(&l), Vec::new(), "{:?}", l.lines);
    let port_on_b1 = |rel| {
        l.lines
            .iter()
            .find(|g| g.relation == rel && g.landing == Some(inner[0]))
            .map(|g| g.points[0].x)
            .ok_or("line")
    };
    assert!(port_on_b1(straight)? < port_on_b1(fan)?);
    Ok(())
}

/// Partner names at the frame of `l` that another line runs through.
fn crossed_frame_names(l: &Layout) -> Vec<(usize, String)> {
    scene::frame_label_rects(l)
        .into_iter()
        .filter(|(li, r)| {
            l.lines.iter().enumerate().any(|(lj, g)| {
                lj != *li
                    && g.drawn().into_iter().any(|piece| {
                        piece
                            .windows(2)
                            .any(|w| layout::segment_meets(w[0], w[1], r))
                    })
            })
        })
        .map(|(li, _)| {
            (
                li,
                l.lines[li]
                    .frame_port
                    .clone()
                    .map(|f| f.2)
                    .unwrap_or_default(),
            )
        })
        .collect()
}

#[test]
fn tags_at_frame_points_a_port_apart_keep_off_the_neighbouring_lines() -> TestResult {
    use whiteboxed::model::MouseholeMode;
    // Three relations from above, each through mouseholes to the same two inner
    // boxes: every branch gets a frame point of its own, three a port apart over
    // each box, each with a tag like `[2] 1/2`.
    let mut p = Project::new();
    let a = p.add_block(None, &spec("A", BlockKind::Component))?;
    let b1 = p.add_block(Some(a), &spec("B1", BlockKind::Component))?;
    let b2 = p.add_block(Some(a), &spec("B2", BlockKind::Component))?;
    p.move_block(b2, Cell::new(1, 0))?;
    let mut rels = Vec::new();
    for name in ["X", "Y", "Z"] {
        let (_, rel) = p.connect_new(
            a,
            Side::Top,
            &spec(name, BlockKind::Component),
            Direction::In,
            "uses",
        )?;
        p.attach(rel, End::A, a, b1, Side::Top)?;
        p.attach(rel, End::A, a, b2, Side::Top)?;
        p.set_mousehole(rel, Some(a), MouseholeMode::Always)?;
        rels.push(rel);
    }
    let l = render(&p, Some(a));
    let mut names: Vec<(f32, String, (bool, usize, f32))> = l
        .lines
        .iter()
        .filter(|g| g.frame_named)
        .filter_map(|g| {
            g.frame_port
                .as_ref()
                .map(|f| (f.0.x, f.2.clone(), g.frame_name))
        })
        .collect();
    names.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(names.len(), 6, "{names:?}");
    assert!(names.iter().all(|n| n.1.contains('/')), "{names:?}");
    assert_eq!(crossed_frame_names(&l), vec![], "{names:?}");
    // Every tag sits in the rows next to the frame; one with no room for its whole
    // width between two stems wraps into number and place.
    for (_, _, (_, row, _)) in &names {
        assert_eq!(*row, 0, "{names:?}");
    }
    assert!(names.iter().any(|n| n.2.2 < PORT_SPACING), "{names:?}");
    Ok(())
}

#[test]
fn a_partner_name_with_no_room_between_the_lines_becomes_its_number() -> TestResult {
    // Three relations from above land on the same inner box: three frame points a
    // port apart. The middle one's long partner name fits between neither pair of
    // stems; it shows its legend number, the others keep their names.
    let mut p = Project::new();
    let a = p.add_block(None, &spec("A", BlockKind::Component))?;
    let inner = p.add_block(Some(a), &spec("Inner", BlockKind::Component))?;
    let mut rels = Vec::new();
    for name in ["Left", "Some Long Partner Name", "Right"] {
        let (_, rel) = p.connect_new(
            a,
            Side::Top,
            &spec(name, BlockKind::Component),
            Direction::In,
            "",
        )?;
        p.attach(rel, End::A, a, inner, Side::Top)?;
        rels.push(rel);
    }
    let l = render(&p, Some(a));
    assert_eq!(crossed_frame_names(&l), vec![]);
    let name_of = |rel: RelationId| {
        l.lines
            .iter()
            .find(|g| g.relation == rel)
            .and_then(|g| g.frame_port.clone())
            .map(|f| f.2)
            .unwrap_or_default()
    };
    assert_eq!(name_of(rels[0]), "Left");
    assert_eq!(name_of(rels[2]), "Right");
    assert!(name_of(rels[1]).starts_with('['), "{}", name_of(rels[1]));
    assert!(
        l.legend
            .iter()
            .any(|e| e.key == name_of(rels[1]) && e.lines.join(" ") == "Some Long Partner Name"),
        "{:?}",
        l.legend
    );
    Ok(())
}

#[test]
fn a_line_under_the_bands_keeps_its_own_label_after_a_line_that_is_not_drawn() -> TestResult {
    // A view line with no port at either end is not drawn. The line set to enter
    // through the bottom after it must still find its own short label at the holes
    // under the band, not the undrawn line's.
    let (mut p, a, _, rel) = fanned_in(&["b1"], &[0], "uses")?;
    p.add_block(Some(a), &spec("Guards", BlockKind::Component).as_band())?;
    p.set_relation_short(rel, "u")?;
    p.set_frame_side(rel, a, Some(Side::Bottom))?;
    let mut view = diagram_view(&p, Some(a));
    let mut undrawn = view.lines.first().ok_or("line")?.clone();
    undrawn.relation = RelationId(9999);
    undrawn.a = ViewEnd::Open;
    undrawn.b = ViewEnd::Open;
    undrawn.short = "x".to_owned();
    view.lines.insert(0, undrawn);
    let l = layout::layout(&p, &view);
    let line = l.lines.iter().find(|g| g.relation == rel).ok_or("line")?;
    assert!(line.mousehole.is_some(), "no hole at the band");
    assert_eq!(line.text, "u");
    Ok(())
}
