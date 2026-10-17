use whiteboxed::layout::{self, ICON_H, LOOK_GAP, Layout};
use whiteboxed::look::{Look, LookId, LookMode};
use whiteboxed::model::{BlockId, BlockKind, BlockSpec, Direction, Project, Side};
use whiteboxed::view::diagram_view;
use whiteboxed::{export, html, scene};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Red, twice as wide as high, with a gradient id that two copies would share.
const RED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20">
<defs><linearGradient id="g"><stop offset="0" stop-color="#ff0000"/></linearGradient></defs>
<rect width="40" height="20" fill="#ff0000"/></svg>"##;

fn render(p: &Project, diagram: Option<BlockId>) -> Layout {
    layout::layout(p, &diagram_view(p, diagram))
}

/// Two connected boxes; `a` wears a red look in `mode`.
fn pair(mode: LookMode) -> Result<(Project, BlockId, BlockId), Box<dyn std::error::Error>> {
    let mut p = Project::new();
    let a = p.add_block(None, &BlockSpec::new("Kafka", BlockKind::Queue))?;
    let (b, _) = p.connect_new(
        a,
        Side::Right,
        &BlockSpec::new("Orders", BlockKind::Component),
        Direction::Out,
        "",
    )?;
    let id = LookId("red".into());
    p.looks.insert(id.clone(), Look::new("Red", mode, RED, 0)?);
    p.blocks.get_mut(&a).ok_or("a")?.look = Some(id);
    Ok((p, a, b))
}

fn size(l: &Layout, id: BlockId) -> Result<(f32, f32), Box<dyn std::error::Error>> {
    let r = l.block(id).ok_or("block")?.rect;
    Ok((r.width(), r.height()))
}

#[test]
fn a_look_keeps_its_proportions() -> TestResult {
    let (p, a, b) = pair(LookMode::Inside)?;
    let l = render(&p, None);
    let (w, h) = size(&l, a)?;
    assert!((w / h - 2.0).abs() < 0.01, "{w}x{h}");
    let pictures: Vec<_> = scene::scene(&l)
        .shapes
        .into_iter()
        .filter_map(|s| match s {
            scene::Shape::Look { rect, .. } => Some(rect),
            _ => None,
        })
        .collect();
    assert_eq!(pictures.len(), 1);
    assert!((pictures[0].width() / pictures[0].height() - 2.0).abs() < 0.01);

    // As an icon the box keeps its own size and grows by the icon.
    let (p, a, _) = pair(LookMode::Icon)?;
    let plain = size(&render(&p, None), b)?;
    let (w, h) = size(&render(&p, None), a)?;
    assert_eq!(w, plain.0);
    assert_eq!(h, plain.1 + ICON_H + LOOK_GAP);

    // With the name below, the picture sits above the text.
    let (p, a, _) = pair(LookMode::Below)?;
    let l = render(&p, None);
    let s = scene::scene(&l);
    let picture = s
        .shapes
        .iter()
        .find_map(|s| match s {
            scene::Shape::Look { rect, .. } => Some(*rect),
            _ => None,
        })
        .ok_or("look")?;
    let name_y = s
        .shapes
        .iter()
        .find_map(|s| match s {
            scene::Shape::Text { text, pos, .. } if text == "Kafka" => Some(pos.y),
            _ => None,
        })
        .ok_or("name")?;
    assert!(picture.max.y < name_y);
    assert!(l.block(a).ok_or("a")?.rect.holds(&picture));
    Ok(())
}

fn duplicate_ids(svg: &str) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    svg.split(" id=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .filter(|id| !seen.insert(id.to_string()))
        .map(str::to_owned)
        .collect()
}

#[test]
fn exports_embed_the_look_with_ids_of_its_own() -> TestResult {
    let (mut p, a, b) = pair(LookMode::Inside)?;
    // A second box with the same look, one level down, for the HTML page.
    let inner = p.add_block(Some(b), &BlockSpec::new("Topic", BlockKind::Queue))?;
    let look = p.blocks.get(&a).and_then(|x| x.look.clone());
    p.blocks.get_mut(&inner).ok_or("inner")?.look = look;
    let svg = export::to_svg(&scene::scene(&render(&p, None)));
    assert!(svg.contains(&format!(r#"id="look{}-g""#, a.0)));
    assert!(!svg.contains("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 40 20\""));
    let page = html::html_doc(&p, "Looks");
    assert_eq!(page.matches(r#"id="look"#).count(), 2);
    assert!(
        duplicate_ids(&page).is_empty(),
        "{:?}",
        duplicate_ids(&page)
    );
    Ok(())
}

#[test]
fn the_png_shows_the_look() -> TestResult {
    let (p, a, _) = pair(LookMode::Inside)?;
    let l = render(&p, None);
    let s = scene::scene(&l);
    let png = export::to_png(&s, 1.0)?;
    let img = image::load_from_memory(&png)?.to_rgba8();
    // The picture starts at the scene's bounds minus the export margin.
    let c = l.block(a).ok_or("a")?.rect.center();
    let (x, y) = (c.x - s.bounds.min.x + 12.0, c.y - s.bounds.min.y + 12.0);
    // Away from the name and caption in the middle.
    let [r, g, bl, _] = img.get_pixel((x + 55.0) as u32, (y + 25.0) as u32).0;
    assert_eq!((r, g, bl), (255, 0, 0));
    Ok(())
}

/// Blue, a 20x40 drawing sitting small in a 100x100 viewBox, as the AI draws them.
const TALL: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<rect x="40" y="30" width="20" height="40" fill="#0000ff"/></svg>"##;

#[test]
fn a_look_is_cropped_to_what_it_draws() -> TestResult {
    let look = Look::new("Postgres", LookMode::Below, TALL, 0)?;
    assert_eq!(look.size(), (20.0, 40.0));
    assert!(
        look.svg().contains(r#"viewBox="40 30 20 40""#),
        "{}",
        look.svg()
    );
    assert!(
        look.svg()
            .starts_with(r#"<svg xmlns="http://www.w3.org/2000/svg""#)
    );
    // Strokes count; a picture read again stays as it is; a tight one is untouched.
    let stroked = TALL.replace(
        r##"fill="#0000ff""##,
        r##"fill="#0000ff" stroke="#000000" stroke-width="2""##,
    );
    assert_eq!(
        Look::new("S", LookMode::Below, &stroked, 0)?.size(),
        (22.0, 42.0)
    );
    let again = Look::new("Postgres", LookMode::Below, look.svg(), 0)?;
    assert_eq!(again.svg(), look.svg());
    let red = Look::new("Red", LookMode::Inside, RED, 0)?;
    assert_eq!(red.size(), (40.0, 20.0));
    Ok(())
}

#[test]
fn lines_reach_the_picture_and_the_framed_name_below() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(
        None,
        &BlockSpec::new("PostgreSQL cluster", BlockKind::Database),
    )?;
    let id = LookId("pg".into());
    p.looks
        .insert(id.clone(), Look::new("Postgres", LookMode::Below, TALL, 0)?);
    p.blocks.get_mut(&a).ok_or("a")?.look = Some(id);
    let mut rels = Vec::new();
    for (side, name) in [
        (Side::Right, "App"),
        (Side::Bottom, "Backup"),
        (Side::Top, "Admin"),
    ] {
        let (_, rel) = p.connect_new(
            a,
            side,
            &BlockSpec::new(name, BlockKind::Component),
            Direction::Out,
            "",
        )?;
        rels.push(rel);
    }
    let l = render(&p, None);
    let g = l.block(a).ok_or("a")?;
    let (picture, caption) = (g.picture.ok_or("picture")?, g.caption.ok_or("caption")?);
    // The picture keeps its size; the name is wider and sticks out under it.
    assert!(
        (picture.width() / picture.height() - 0.5).abs() < 0.01,
        "{picture:?}"
    );
    assert!(caption.width() > picture.width());
    assert!((g.rect.width() - caption.width()).abs() < 0.01);
    assert!(g.rect.holds(&picture) && g.rect.holds(&caption));
    let start = |rel| -> Result<whiteboxed::geom::Pos, Box<dyn std::error::Error>> {
        Ok(l.lines
            .iter()
            .find(|x| x.relation == rel)
            .ok_or("line")?
            .points[0])
    };
    let (right, below, above) = (start(rels[0])?, start(rels[1])?, start(rels[2])?);
    assert!(
        (right.x - picture.max.x).abs() < 0.01
            && right.y > picture.min.y
            && right.y < picture.max.y
    );
    assert!(
        (below.y - caption.max.y).abs() < 0.01,
        "{below:?} {caption:?}"
    );
    assert!(
        (above.y - picture.min.y).abs() < 0.01,
        "{above:?} {picture:?}"
    );
    // The frame is drawn around the name.
    let s = scene::scene(&l);
    assert!(
        s.shapes
            .iter()
            .any(|sh| matches!(sh, scene::Shape::Rect { rect, .. } if *rect == caption))
    );
    // More lines on a side: the picture grows as a whole, never stretched.
    for i in 0..3 {
        p.connect_new(
            a,
            Side::Right,
            &BlockSpec::new(&format!("S{i}"), BlockKind::Component),
            Direction::Out,
            "",
        )?;
    }
    let grown = render(&p, None)
        .block(a)
        .ok_or("a")?
        .picture
        .ok_or("picture")?;
    assert!(
        grown.height() >= 5.0 * layout::PORT_SPACING - 0.01,
        "{grown:?}"
    );
    assert!(
        (grown.width() / grown.height() - 0.5).abs() < 0.01,
        "{grown:?}"
    );
    Ok(())
}

/// A plug twice as wide as high, a stencil: it takes the line's colour.
const PLUG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 10">
<path d="M0 0h20v10H0z" fill="currentColor"/></svg>"##;

/// The pair, with the relation wearing a dashed line look 3 wide with a plug head.
fn looked_pair(
    direction: Direction,
) -> Result<(Project, BlockId, BlockId), Box<dyn std::error::Error>> {
    use whiteboxed::look::{LineLook, LineLookSpec, Picture};
    use whiteboxed::model::LinePattern;
    let (mut p, a, b) = pair(LookMode::Inside)?;
    let plug = LookId("plug".into());
    p.pictures
        .insert(plug.clone(), Picture::new("Plug", PLUG, 0)?);
    let spec = LineLookSpec {
        width: 3.0,
        dash: LinePattern::Dashed,
        head: Some(plug),
        tail: None,
    };
    let async_ = LookId("async".into());
    p.line_looks
        .insert(async_.clone(), LineLook::new("async", spec, 0)?);
    for r in p.relations.values_mut() {
        r.look = Some(async_.clone());
        r.direction = direction;
    }
    Ok((p, a, b))
}

fn end_pictures(s: &scene::Scene) -> Vec<(whiteboxed::geom::Rect, f32, String)> {
    s.shapes
        .iter()
        .filter_map(|s| match s {
            scene::Shape::Look {
                rect, angle, svg, ..
            } if rect.height() < 20.0 => Some((*rect, *angle, svg.to_string())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_line_look_sets_the_stroke_and_puts_its_picture_at_the_head() -> TestResult {
    let (p, a, b) = looked_pair(Direction::Out)?;
    let l = render(&p, None);
    let s = scene::scene(&l);
    let line = l.lines.first().ok_or("line")?;
    let tip = *line.points.last().ok_or("tip")?;
    let start = line.points[0];
    // The stroke is the look's.
    // (The legend below the diagram shows a sample of the look too: left out here.)
    let in_diagram = |r: &whiteboxed::geom::Rect| r.max.y <= l.legend_at.map_or(f32::MAX, |a| a.y);
    let strokes: Vec<_> = s
        .shapes
        .iter()
        .filter_map(|s| match s {
            scene::Shape::Polyline { stroke, points }
                if points
                    .iter()
                    .all(|q| q.y <= l.legend_at.map_or(f32::MAX, |a| a.y)) =>
            {
                Some((*stroke, points.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(strokes.len(), 1);
    let (stroke, points) = &strokes[0];
    assert_eq!(stroke.width, 3.0);
    assert!(stroke.dash.is_some());
    // The head picture: END_H high, twice as wide, its right edge at the tip, pointing
    // right along the horizontal line; the line stops where the picture begins.
    let ends: Vec<_> = end_pictures(&s)
        .into_iter()
        .filter(|(r, _, _)| in_diagram(r))
        .collect();
    assert_eq!(ends.len(), 1);
    let (rect, angle, svg) = &ends[0];
    assert!((rect.height() - scene::END_H).abs() < 0.01);
    assert!((rect.width() - 2.0 * scene::END_H).abs() < 0.01);
    assert!((rect.max.x - tip.x).abs() < 0.01 && (rect.center().y - tip.y).abs() < 0.01);
    assert!(angle.abs() < 1e-4);
    assert!((points.last().ok_or("end")?.x - rect.min.x).abs() < 0.01);
    assert!((points[0].x - start.x).abs() < 0.01);
    // The stencil is tinted in the line's colour (the default here).
    assert!(
        svg.contains(&format!(r#"color="{}""#, scene::LINE)),
        "{svg}"
    );
    // No plain arrowhead besides it.
    assert!(
        !s.shapes
            .iter()
            .any(|s| matches!(s, scene::Shape::Polygon { points, .. } if points.len() == 3))
    );

    // The arrow the other way: the picture sits at end A and points left.
    let (p, _, _) = looked_pair(Direction::In)?;
    let l = render(&p, None);
    let s = scene::scene(&l);
    let ends: Vec<_> = end_pictures(&s)
        .into_iter()
        .filter(|(r, _, _)| r.max.y <= l.legend_at.map_or(f32::MAX, |a| a.y))
        .collect();
    assert_eq!(ends.len(), 1);
    let (rect, angle, _) = &ends[0];
    assert!((rect.min.x - l.lines[0].points[0].x).abs() < 0.01);
    assert!((angle.abs() - std::f32::consts::PI).abs() < 1e-3, "{angle}");
    // Without an arrow, the end gets the tail picture, which this look lacks.
    let (p, _, _) = looked_pair(Direction::Undirected)?;
    let l = render(&p, None);
    assert!(
        end_pictures(&scene::scene(&l))
            .iter()
            .all(|(r, _, _)| r.max.y > l.legend_at.map_or(f32::MAX, |a| a.y))
    );

    // Exports turn the picture and keep the tint; the PNG renders.
    let svg = export::to_svg(&s);
    assert!(
        svg.contains("rotate(180.00") || svg.contains("rotate(-180.00"),
        "{svg}"
    );
    assert!(svg.contains(r##"color="#444b55""##));
    export::to_png(&s, 1.0)?;
    let _ = (a, b);
    Ok(())
}

#[test]
fn lines_bundle_only_with_the_same_look() -> TestResult {
    let (mut p, a, b) = looked_pair(Direction::Out)?;
    let plain = p.connect_existing(
        a,
        Side::Right,
        b,
        Direction::Out,
        "also",
        whiteboxed::model::Placement::Keep,
    )?;
    let view = whiteboxed::view::bundle(&diagram_view(&p, None));
    assert_eq!(view.lines.len(), 2);
    let same = *p.relations.keys().next().ok_or("first")?;
    p.relations.get_mut(&plain).ok_or("plain")?.look =
        p.relations.get(&same).and_then(|r| r.look.clone());
    let view = whiteboxed::view::bundle(&diagram_view(&p, None));
    assert_eq!(view.lines.len(), 1);
    Ok(())
}

/// A portal: a blue ring, as high as wide.
const PORTAL_IN: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
<circle cx="5" cy="5" r="4" fill="none" stroke="#0000ff" stroke-width="2"/></svg>"##;
/// The other portal: an orange ring.
const PORTAL_OUT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
<circle cx="5" cy="5" r="4" fill="none" stroke="#ff8800" stroke-width="2"/></svg>"##;

#[test]
fn portals_replace_the_arches_in_at_the_tail_and_out_at_the_head() -> TestResult {
    use whiteboxed::look::Picture;
    use whiteboxed::model::{Cell, MouseholeMode, Placement};
    let mut p = Project::new();
    let a = p.add_block(None, &BlockSpec::new("A", BlockKind::Component))?;
    let b = p.add_block(None, &BlockSpec::new("B", BlockKind::Component))?;
    p.move_block(b, Cell::new(4, 0))?;
    for col in 1..=3 {
        let f = p.add_block(
            None,
            &BlockSpec::new(&format!("F{col}"), BlockKind::Component),
        )?;
        p.move_block(f, Cell::new(col, 0))?;
    }
    let rel = p.connect_existing(a, Side::Right, b, Direction::Out, "", Placement::Keep)?;
    p.set_mousehole(rel, None, MouseholeMode::Always)?;
    let (into, out) = (LookId("in".into()), LookId("out".into()));
    p.pictures
        .insert(into.clone(), Picture::new("In", PORTAL_IN, 0)?);
    p.pictures
        .insert(out.clone(), Picture::new("Out", PORTAL_OUT, 0)?);
    // Without portals: two arches, no pictures.
    let l = render(&p, None);
    assert!(end_pictures(&scene::scene(&l)).is_empty());
    p.set_portals(
        Some((into.clone(), p.pictures.get(&into).ok_or("in")?.clone())),
        Some((out.clone(), p.pictures.get(&out).ok_or("out")?.clone())),
    )?;
    let l = render(&p, None);
    let m = l.lines[0].mousehole.as_ref().ok_or("holes")?.clone();
    let s = scene::scene(&l);
    let holes = end_pictures(&s);
    assert_eq!(holes.len(), 2);
    // Both as high as the arch was, centred on the holes, each turned away from its
    // port like the arch's round side.
    for (rect, angle, _) in &holes {
        assert!((rect.height() - 2.0 * scene::HOLE_R).abs() < 0.01);
        let away = if rect.center().dist(m.a) < 0.01 {
            0.0
        } else {
            std::f32::consts::PI
        };
        assert!((angle - away).abs() < 1e-3, "{angle}");
    }
    let at = |c: whiteboxed::geom::Pos| {
        holes
            .iter()
            .find(|(r, _, _)| r.center().dist(c) < 0.01)
            .map(|(_, _, svg)| svg.clone())
    };
    // The arrow points at B: in at A's hole, out at B's.
    assert!(at(m.a).ok_or("hole a")?.contains("#0000ff"));
    assert!(at(m.b).ok_or("hole b")?.contains("#ff8800"));
    // No arches left.
    assert!(
        !s.shapes
            .iter()
            .any(|s| matches!(s, scene::Shape::Polygon { points, .. } if points.len() > 3))
    );
    // The exported ids of the portals and of the end pictures stay apart, even when
    // the pictures define the same id.
    {
        use whiteboxed::look::{LineLook, LineLookSpec};
        let mut q = p.clone();
        let shaded = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
<defs><linearGradient id="g"><stop offset="0" stop-color="#0f0"/></linearGradient></defs>
<rect width="10" height="10" fill="url(#g)"/></svg>"##;
        let head = LookId("head".into());
        q.pictures
            .insert(head.clone(), Picture::new("Head", shaded, 0)?);
        q.pictures
            .insert(into.clone(), Picture::new("In", shaded, 1)?);
        let spec = LineLookSpec {
            head: Some(head),
            ..LineLookSpec::default()
        };
        let look = LookId("shaded".into());
        q.line_looks
            .insert(look.clone(), LineLook::new("shaded", spec, 0)?);
        q.relations.get_mut(&rel).ok_or("rel")?.look = Some(look);
        let svg = export::to_svg(&scene::scene(&render(&q, None)));
        assert!(duplicate_ids(&svg).is_empty(), "{:?}", duplicate_ids(&svg));
    }

    // The arrow the other way swaps them.
    p.relations.get_mut(&rel).ok_or("rel")?.direction = Direction::In;
    let l = render(&p, None);
    let m = l.lines[0].mousehole.as_ref().ok_or("holes")?.clone();
    let holes = end_pictures(&scene::scene(&l));
    let at = |c: whiteboxed::geom::Pos| {
        holes
            .iter()
            .find(|(r, _, _)| r.center().dist(c) < 0.01)
            .map(|(_, _, svg)| svg.clone())
    };
    assert!(at(m.a).ok_or("hole a")?.contains("#ff8800"));
    assert!(at(m.b).ok_or("hole b")?.contains("#0000ff"));
    Ok(())
}

#[test]
fn the_legend_shows_a_sample_of_every_line_look_in_use() -> TestResult {
    let (p, _, _) = looked_pair(Direction::Out)?;
    let l = render(&p, None);
    assert_eq!(l.legend.len(), 0);
    assert_eq!(
        l.legend_looks
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["async"]
    );
    let at = l.legend_at.ok_or("legend position")?;
    let s = scene::scene(&l);
    // The sample: a dashed stroke 3 wide pointing right, its plug at the right end,
    // then the name; all of it inside the picture.
    let sample_y = at.y + layout::LEGEND_LINE * 2.0;
    let ends: Vec<_> = end_pictures(&s)
        .into_iter()
        .filter(|(r, _, _)| r.center().y > at.y)
        .collect();
    assert_eq!(ends.len(), 1);
    let (rect, angle, _) = &ends[0];
    assert!(angle.abs() < 1e-4);
    assert!((rect.max.x - (at.x + scene::SAMPLE_W)).abs() < 0.01);
    assert!(
        (rect.center().y - sample_y).abs() < 0.01,
        "{}",
        rect.center().y
    );
    let name = s
        .shapes
        .iter()
        .find_map(|s| match s {
            scene::Shape::Text { text, pos, .. } if text == "async" => Some(*pos),
            _ => None,
        })
        .ok_or("name")?;
    assert!(name.x > rect.max.x && (name.y - sample_y).abs() < 0.01);
    assert!(s.bounds.max.y >= name.y + 8.0);
    Ok(())
}

/// The pair with a box inside `a`, so `a` has a whitebox; `a` wears the red look.
fn whitebox(mode: LookMode) -> Result<(Project, BlockId), Box<dyn std::error::Error>> {
    let (mut p, a, _) = pair(mode)?;
    p.add_block(Some(a), &BlockSpec::new("Inner", BlockKind::Component))?;
    Ok((p, a))
}

fn frame_rect(s: &scene::Scene) -> bool {
    s.shapes.iter().any(|s| {
        matches!(s, scene::Shape::Rect { stroke: Some(st), fill: None, .. } if st.dash.is_some() && st.color == scene::FRAME)
    })
}

fn stretched(s: &scene::Scene) -> Vec<whiteboxed::geom::Rect> {
    s.shapes
        .iter()
        .filter_map(|s| match s {
            scene::Shape::Look {
                rect,
                stretch: true,
                ..
            } => Some(*rect),
            _ => None,
        })
        .collect()
}

fn title_at(s: &scene::Scene) -> Option<(whiteboxed::geom::Pos, scene::Align)> {
    s.shapes.iter().find_map(|s| match s {
        scene::Shape::Text {
            text, pos, align, ..
        } if text == "Kafka (whitebox)" => Some((*pos, *align)),
        _ => None,
    })
}

#[test]
fn a_whitebox_frame_is_its_look_stretched_as_a_nine_slice() -> TestResult {
    let (p, a) = whitebox(LookMode::Inside)?;
    let l = render(&p, Some(a));
    let frame = l.frame.ok_or("frame")?;
    let fl = l.frame_look.as_ref().ok_or("frame look")?;
    // The corners: a quarter of the picture drawn at the plain box height.
    let (cw, ch) = fl.corner;
    assert!((ch - layout::MIN_H / 4.0).abs() < 0.01 && (cw - 2.0 * ch).abs() < 0.01);
    let s = scene::scene(&l);
    assert!(!frame_rect(&s), "the dashed frame is gone");
    let slices = stretched(&s);
    assert_eq!(slices.len(), 9);
    // Together they cover the frame exactly; the corner slices keep their size.
    let area: f32 = slices.iter().map(|r| r.width() * r.height()).sum();
    assert!((area - frame.width() * frame.height()).abs() < 1.0);
    assert!(
        slices
            .iter()
            .all(|r| frame.contains(r.min) && frame.contains(r.max))
    );
    let corner = slices
        .iter()
        .find(|r| r.min == frame.min)
        .ok_or("top left corner")?;
    assert!((corner.width() - cw).abs() < 0.01 && (corner.height() - ch).abs() < 0.01);
    // The title stays inside, top left.
    let (pos, align) = title_at(&s).ok_or("title")?;
    assert!(pos.y < frame.min.y + 30.0 && align == scene::Align::Left);
    assert!(l.frame_caption.is_none());
    // Exports stretch the slices; the PNG renders; ids stay unique.
    let svg = export::to_svg(&s);
    assert_eq!(svg.matches(r#"preserveAspectRatio="none""#).count(), 9);
    assert!(duplicate_ids(&svg).is_empty(), "{:?}", duplicate_ids(&svg));
    export::to_png(&s, 1.0)?;

    // With the name below, the title sits in a caption under the frame.
    let (p, a) = whitebox(LookMode::Below)?;
    let l = render(&p, Some(a));
    let frame = l.frame.ok_or("frame")?;
    let caption = l.frame_caption.ok_or("caption")?;
    assert!(caption.min.y > frame.max.y);
    assert!(l.bounds.max.y >= caption.max.y);
    assert!((caption.center().x - frame.center().x).abs() < 0.01);
    let s = scene::scene(&l);
    assert_eq!(stretched(&s).len(), 9);
    let (pos, align) = title_at(&s).ok_or("title")?;
    assert!(caption.contains(pos) && align == scene::Align::Center);

    // An icon leaves the frame plain and sits next to the title.
    let (p, a) = whitebox(LookMode::Icon)?;
    let l = render(&p, Some(a));
    let frame = l.frame.ok_or("frame")?;
    let s = scene::scene(&l);
    assert!(frame_rect(&s));
    assert!(stretched(&s).is_empty());
    let icon = s
        .shapes
        .iter()
        .find_map(|s| match s {
            scene::Shape::Look { rect, .. } if rect.min.y < frame.min.y + 30.0 => Some(*rect),
            _ => None,
        })
        .ok_or("icon")?;
    assert!((icon.height() - scene::FRAME_ICON_H).abs() < 0.01);
    let (pos, _) = title_at(&s).ok_or("title")?;
    assert!(pos.x > icon.max.x);
    assert!(l.frame_caption.is_none());

    // The context view has no frame and no frame look.
    let l = render(&p, None);
    assert!(l.frame_look.is_none() && l.frame_caption.is_none());
    Ok(())
}
