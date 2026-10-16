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
