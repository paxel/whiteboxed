use whiteboxed::model::{
    BlockKind, BlockSpec, Cell, Direction, Placement, Project, ScoreLimits, Side,
};
use whiteboxed::score::{self, Level};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn component(name: &str) -> BlockSpec {
    BlockSpec::new(name, BlockKind::Component)
}

#[test]
fn a_small_diagram_is_green() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    p.connect_new(a, Side::Right, &component("B"), Direction::Out, "")?;
    let s = score::of(&p, None);
    assert_eq!((s.boxes, s.crossings, s.level()), (2, 0, Level::Green));
    assert!(s.causes().is_empty());
    assert!(s.summary().contains("good"));
    Ok(())
}

#[test]
fn many_boxes_turn_yellow_then_red() -> TestResult {
    let mut p = Project::new();
    for i in 0..7 {
        p.add_block(None, &component(&format!("B{i}")))?;
    }
    assert_eq!(score::of(&p, None).boxes_level, Level::Green);
    p.add_block(None, &component("B7"))?;
    assert_eq!(score::of(&p, None).boxes_level, Level::Yellow);
    p.add_block(None, &component("B8"))?;
    p.add_block(None, &component("B9"))?;
    let s = score::of(&p, None);
    assert_eq!(s.level(), Level::Red);
    let causes = s.causes();
    assert_eq!(causes.len(), 1);
    assert_eq!(causes[0].what, "10 boxes");
    assert!(causes[0].help.contains("Group"));
    // The project's own limits decide.
    p.set_score_limits(ScoreLimits {
        boxes: (20, 30),
        ..ScoreLimits::default()
    });
    assert_eq!(score::of(&p, None).level(), Level::Green);
    Ok(())
}

#[test]
fn crossings_and_a_busy_box_are_named() -> TestResult {
    let mut p = Project::new();
    let k = BlockKind::Component;
    let mut ids = Vec::new();
    for (i, (col, row)) in [(0, 1), (2, 1), (1, 0), (1, 2), (1, 1)]
        .into_iter()
        .enumerate()
    {
        let id = p.add_block(None, &BlockSpec::new(&format!("B{i}"), k))?;
        p.move_block(id, Cell::new(col, row))?;
        ids.push(id);
    }
    p.connect_existing(
        ids[0],
        Side::Right,
        ids[1],
        Direction::Out,
        "",
        Placement::Keep,
    )?;
    p.connect_existing(
        ids[2],
        Side::Bottom,
        ids[3],
        Direction::Out,
        "",
        Placement::Keep,
    )?;
    for side in [
        Side::Top,
        Side::Bottom,
        Side::Left,
        Side::Right,
        Side::Top,
        Side::Bottom,
    ] {
        p.connect_new(
            ids[4],
            side,
            &component(&format!("N{side:?}{}", p.blocks.len())),
            Direction::Out,
            "",
        )?;
    }
    let s = score::of(&p, None);
    assert!(s.crossings >= 1, "{s:?}");
    assert_eq!(
        s.busiest.as_ref().map(|b| (b.1.as_str(), b.2)),
        Some(("B4", 6))
    );
    assert_eq!(s.lines_level, Level::Yellow);
    let what: Vec<String> = s.causes().into_iter().map(|c| c.what).collect();
    assert!(what.iter().any(|w| w.contains("crossing")), "{what:?}");
    assert!(what.iter().any(|w| w == "B4 has 6 lines"), "{what:?}");
    Ok(())
}

#[test]
fn a_doodle_page_counts_shapes_but_not_groups() -> TestResult {
    let mut p = Project::new_doodle();
    let page = p.pages()[0];
    let mut shapes = Vec::new();
    for i in 0..8 {
        shapes.push(p.add_block(Some(page), &component(&format!("S{i}")))?);
    }
    p.add_group(&shapes[..2], "Auth", None)?;
    let s = score::of(&p, Some(page));
    assert!(s.doodle);
    assert_eq!((s.boxes, s.boxes_level), (8, Level::Yellow));
    let causes = s.causes();
    assert_eq!(causes[0].what, "8 shapes");
    // Help for a sketch, without arc42 advice.
    assert!(causes[0].help.contains("page"));
    assert!(!causes[0].help.contains("arc42"));
    assert!(s.explained().contains(&causes[0].help));
    // The file's own limits decide here too.
    p.set_score_limits(ScoreLimits {
        boxes: (20, 30),
        ..ScoreLimits::default()
    });
    let s = score::of(&p, Some(page));
    assert_eq!(s.level(), Level::Green);
    assert!(s.summary().contains("8 shapes"));
    Ok(())
}

#[test]
fn lines_through_mouseholes_are_a_measure_of_their_own() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let b = p.add_block(None, &component("B"))?;
    p.move_block(b, Cell::new(4, 0))?;
    for col in 1..=3 {
        let f = p.add_block(None, &component(&format!("F{col}")))?;
        p.move_block(f, Cell::new(col, 0))?;
    }
    p.set_bundle(false);
    for text in ["a", "b", "c"] {
        let rel = p.connect_existing(a, Side::Right, b, Direction::Out, text, Placement::Keep)?;
        // The outer lines only graze the boxes in between: told to go under.
        p.set_mousehole(rel, None, whiteboxed::model::MouseholeMode::Always)?;
    }
    let s = score::of(&p, None);
    assert_eq!((s.mouseholes, s.mouseholes_level), (3, Level::Yellow));
    assert_eq!(s.level(), Level::Yellow);
    let causes = s.causes();
    assert_eq!(causes.len(), 1);
    assert_eq!(causes[0].what, "3 lines through mouseholes");
    assert!(causes[0].help.contains("never"));
    p.set_score_limits(ScoreLimits {
        mouseholes: (20, 30),
        ..ScoreLimits::default()
    });
    assert_eq!(score::of(&p, None).level(), Level::Green);
    Ok(())
}

#[test]
fn kinds_of_line_are_a_measure_of_their_own() -> TestResult {
    use whiteboxed::look::{LineLook, LineLookSpec, LookId};
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let b = p.add_block(None, &component("B"))?;
    p.set_bundle(false);
    for name in ["async", "sync", "data"] {
        let id = LookId(name.into());
        p.line_looks
            .insert(id.clone(), LineLook::new(name, LineLookSpec::default(), 0)?);
        let rel = p.connect_existing(a, Side::Right, b, Direction::Out, name, Placement::Keep)?;
        p.relations.get_mut(&rel).ok_or("rel")?.look = Some(id);
    }
    let s = score::of(&p, None);
    assert_eq!((s.line_looks, s.line_looks_level), (3, Level::Yellow));
    assert_eq!(s.level(), Level::Yellow);
    let causes = s.causes();
    assert_eq!(causes.len(), 1);
    assert_eq!(causes[0].what, "3 kinds of line");
    // Two lines of the same kind count once.
    let same = *p.relations.keys().next().ok_or("first")?;
    let look = p.relations.get(&same).and_then(|r| r.look.clone());
    for r in p.relations.values_mut() {
        r.look = look.clone();
    }
    assert_eq!(score::of(&p, None).line_looks, 1);
    p.set_score_limits(ScoreLimits {
        line_looks: (1, 2),
        ..ScoreLimits::default()
    });
    assert_eq!(score::of(&p, None).level(), Level::Yellow);
    Ok(())
}
