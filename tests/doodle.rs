use whiteboxed::editor::{Editor, Popup};
use whiteboxed::layout;
use whiteboxed::model::{BlockKind, BlockSpec, Direction, ModelError, Project, ProjectKind, Side};
use whiteboxed::persist;
use whiteboxed::view::diagram_view;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn shape(text: &str) -> BlockSpec {
    BlockSpec::new(text, BlockKind::Component)
}

#[test]
fn a_doodle_has_pages_with_shapes_free_of_arc42_rules() -> TestResult {
    let mut p = Project::new_doodle();
    assert_eq!(p.kind, ProjectKind::Doodle);
    let pages = p.pages();
    assert_eq!(pages.len(), 1);
    let page = pages[0];
    // The same text twice, over two lines, and a person on a page.
    let a = p.add_block(Some(page), &shape("Check token\nvalid?"))?;
    let b = p.add_block(Some(page), &shape("Check token\nvalid?"))?;
    p.add_block(Some(page), &BlockSpec::new("User", BlockKind::Person))?;
    assert_eq!(p.block(a)?.name, "Check token\nvalid?");
    p.connect_existing(
        a,
        Side::Right,
        b,
        Direction::Out,
        "",
        whiteboxed::model::Placement::Keep,
    )?;
    // No shapes at the top, none inside shapes, no open ends, no automatic name.
    assert_eq!(
        p.add_block(None, &shape("X")),
        Err(ModelError::PageOutOfPlace)
    );
    assert_eq!(
        p.add_block(Some(a), &shape("X")),
        Err(ModelError::PageOutOfPlace)
    );
    assert_eq!(
        p.add_stub(a, Side::Top, Direction::Out, ""),
        Err(ModelError::NoOpenEnds)
    );
    assert_eq!(p.auto_name(), None);
    // Pages: names stay unique, the last one stays.
    let second = p.add_page("Network")?;
    assert!(p.add_page("network").is_err());
    p.delete_block(second)?;
    assert_eq!(p.delete_block(page), Err(ModelError::LastPage));
    Ok(())
}

#[test]
fn arc42_projects_keep_their_rules() -> TestResult {
    let mut p = Project::new();
    assert_eq!(p.add_page("Page"), Err(ModelError::PageOutOfPlace));
    let shop = p.add_block(None, &shape("Shop"))?;
    assert_eq!(
        p.add_block(Some(shop), &BlockSpec::new("User", BlockKind::Person)),
        Err(ModelError::NeighbourBelowContext("person"))
    );
    assert_eq!(
        p.add_block(None, &shape("shop")),
        Err(ModelError::DuplicateName("shop".into()))
    );
    assert_eq!(
        p.add_block(None, &shape("two\nlines")),
        Err(ModelError::ControlCharacter)
    );
    Ok(())
}

#[test]
fn a_doodle_is_saved_with_its_kind_and_checked_on_load() -> TestResult {
    let mut p = Project::new_doodle();
    let page = p.pages()[0];
    p.add_block(Some(page), &shape("A"))?;
    let yaml = persist::to_yaml(&p)?;
    assert!(yaml.starts_with("format: 1\nkind: doodle\n"), "{yaml}");
    let back = persist::from_yaml(&yaml)?;
    assert_eq!(back, p);
    // An arc42 file does not mention its kind.
    assert!(!persist::to_yaml(&Project::new())?.contains("kind:"));
    // A page in an arc42 file is refused.
    let broken = yaml.replace("kind: doodle\n", "");
    assert!(persist::from_yaml(&broken).is_err());
    Ok(())
}

#[test]
fn a_doodle_page_has_no_frame_and_no_type_captions() -> TestResult {
    let mut p = Project::new_doodle();
    let page = p.pages()[0];
    p.add_block(Some(page), &shape("A"))?;
    let l = layout::layout(&p, &diagram_view(&p, Some(page)));
    assert!(l.frame.is_none() && l.title.is_none());
    assert!(!l.captions);
    Ok(())
}

#[test]
fn the_editor_always_shows_a_page() -> TestResult {
    let mut e = Editor::new_doodle(None);
    let first = e.project.pages()[0];
    assert_eq!(e.diagram, Some(first));
    e.go_up();
    assert_eq!(e.diagram, Some(first));
    e.add_page();
    let second = e.diagram.ok_or("no page")?;
    assert_ne!(second, first);
    assert_eq!(e.project.block(second)?.name, "Page 2");
    e.request_delete(second);
    if e.popup.is_some() {
        e.confirm();
    }
    assert_eq!(e.diagram, Some(first));
    assert!(e.scores().is_empty());
    // Shapes do not open into anything.
    let a = e.project.add_block(Some(first), &shape("A"))?;
    e.open_diagram(Some(a));
    assert_eq!(e.diagram, Some(first));
    assert!(!matches!(e.popup, Some(Popup::Restore(_))));
    Ok(())
}

#[test]
fn doodle_shapes_have_colours_and_stay_in_doodles() -> TestResult {
    use whiteboxed::model::Rgb;
    let mut p = Project::new_doodle();
    let page = p.pages()[0];
    let spec = BlockSpec {
        color: Some(Rgb(0xf4, 0xa3, 0xa3)),
        ..BlockSpec::new("valid?", BlockKind::Diamond)
    };
    let d = p.add_block(Some(page), &spec)?;
    assert_eq!(p.block(d)?.color, Some(Rgb(0xf4, 0xa3, 0xa3)));
    let back = persist::from_yaml(&persist::to_yaml(&p)?)?;
    assert_eq!(back.block(d)?.color, Some(Rgb(0xf4, 0xa3, 0xa3)));
    let l = layout::layout(&p, &diagram_view(&p, Some(page)));
    assert_eq!(l.block(d).ok_or("diamond")?.fill, Rgb(0xf4, 0xa3, 0xa3));
    // Not in arc42 projects.
    let mut arc = Project::new();
    assert_eq!(
        arc.add_block(None, &BlockSpec::new("X", BlockKind::Cloud)),
        Err(ModelError::DoodleOnly("cloud"))
    );
    Ok(())
}

#[test]
fn shapes_grow_with_their_lines_and_lines_reach_their_outline() -> TestResult {
    let mut p = Project::new_doodle();
    let page = p.pages()[0];
    let one = p.add_block(Some(page), &BlockSpec::new("One", BlockKind::Rounded))?;
    let three = p.add_block(
        Some(page),
        &BlockSpec::new("a\nb\nc\nd\ne", BlockKind::Rounded),
    )?;
    let dia = p.add_block(Some(page), &BlockSpec::new("ok?", BlockKind::Diamond))?;
    let circle = p.add_block(Some(page), &BlockSpec::new("start", BlockKind::Circle))?;
    p.move_block(circle, whiteboxed::model::Cell::new(0, 2))?;
    p.move_block(dia, whiteboxed::model::Cell::new(2, 2))?;
    let rel = p.connect_existing(
        circle,
        Side::Right,
        dia,
        Direction::Out,
        "",
        whiteboxed::model::Placement::Keep,
    )?;
    let l = layout::layout(&p, &diagram_view(&p, Some(page)));
    let h = |b| -> Result<f32, String> { Ok(l.block(b).ok_or("box")?.rect.height()) };
    assert!(h(three)? > h(one)?);
    let d = l.block(dia).ok_or("diamond")?.rect;
    let c = l.block(circle).ok_or("circle")?.rect;
    let line = l.lines.iter().find(|g| g.relation == rel).ok_or("line")?;
    let (start, end) = (line.points[0], line.points[line.points.len() - 1]);
    // The straight line meets the circle's right edge and the diamond's left tip.
    assert!((start.x - c.max.x).abs() < 1.0, "{start:?} {c:?}");
    assert!((end.x - d.min.x).abs() < 1.0, "{end:?} {d:?}");
    Ok(())
}

#[test]
fn an_off_centre_line_reaches_into_a_round_shape() -> TestResult {
    let mut p = Project::new_doodle();
    let page = p.pages()[0];
    let hub = p.add_block(Some(page), &BlockSpec::new("hub", BlockKind::Circle))?;
    let mut lines = Vec::new();
    for i in 0..3 {
        let (_, r) = p.connect_new(
            hub,
            Side::Right,
            &BlockSpec::new(&format!("t{i}"), BlockKind::Component),
            Direction::Out,
            "",
        )?;
        lines.push(r);
    }
    let l = layout::layout(&p, &diagram_view(&p, Some(page)));
    let c = l.block(hub).ok_or("hub")?.rect;
    let starts: Vec<f32> = lines
        .iter()
        .filter_map(|r| l.lines.iter().find(|g| g.relation == *r))
        .map(|g| g.points[0].x)
        .collect();
    // Off the middle of the side, a line starts inside the box, on the circle.
    let centre = c.center();
    for r in &lines {
        let g = l.lines.iter().find(|g| g.relation == *r).ok_or("line")?;
        let p0 = g.points[0];
        let dx = (p0.x - centre.x) / (c.width() / 2.0);
        let dy = (p0.y - centre.y) / (c.height() / 2.0);
        assert!(
            (dx * dx + dy * dy - 1.0).abs() < 0.02,
            "{p0:?} is not on the outline"
        );
    }
    assert!(starts.iter().any(|x| *x < c.max.x - 2.0), "{starts:?}");
    Ok(())
}

#[test]
fn lines_can_be_dashed_or_dotted() -> TestResult {
    use whiteboxed::model::LinePattern;
    let mut p = Project::new_doodle();
    let page = p.pages()[0];
    let a = p.add_block(Some(page), &shape("A"))?;
    let (_, r) = p.connect_new(a, Side::Right, &shape("B"), Direction::Out, "async")?;
    p.set_relation_pattern(r, LinePattern::Dotted)?;
    let yaml = persist::to_yaml(&p)?;
    assert!(yaml.contains("pattern: dotted"), "{yaml}");
    assert_eq!(
        persist::from_yaml(&yaml)?.relation(r)?.pattern,
        LinePattern::Dotted
    );
    let l = layout::layout(&p, &diagram_view(&p, Some(page)));
    let svg = whiteboxed::export::to_svg(&whiteboxed::scene::scene(&l));
    assert!(svg.contains(r#"stroke-dasharray="1.5 3.5""#));
    p.set_relation_pattern(r, LinePattern::Solid)?;
    assert!(!persist::to_yaml(&p)?.contains("pattern:"));
    Ok(())
}

#[test]
fn lines_with_different_patterns_are_not_bundled() -> TestResult {
    use whiteboxed::model::{LinePattern, Placement};
    let mut p = Project::new_doodle();
    let page = p.pages()[0];
    let a = p.add_block(Some(page), &shape("A"))?;
    let (b, _) = p.connect_new(a, Side::Right, &shape("B"), Direction::Out, "calls")?;
    let health = p.connect_existing(b, Side::Left, a, Direction::Out, "health", Placement::Keep)?;
    assert_eq!(
        layout::layout(&p, &diagram_view(&p, Some(page)))
            .lines
            .len(),
        1
    );
    p.set_relation_pattern(health, LinePattern::Dotted)?;
    assert_eq!(
        layout::layout(&p, &diagram_view(&p, Some(page)))
            .lines
            .len(),
        2
    );
    Ok(())
}
