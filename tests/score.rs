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
