use std::time::{Duration, Instant};

use whiteboxed::doc::DocFormat;
use whiteboxed::editor::{ConnectMode, Editor, OpenEnd, Popup, Target};
use whiteboxed::geom::Pos;
use whiteboxed::hit::{self, Hit};
use whiteboxed::model::{BlockId, BlockKind, Cell, Direction, Side};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn add_first(e: &mut Editor, name: &str) -> Result<BlockId, Box<dyn std::error::Error>> {
    e.start_add_block();
    if let Some(Popup::AddBlock(form)) = &mut e.popup {
        form.name = name.into();
    }
    e.confirm();
    e.selected.ok_or_else(|| "no selection".into())
}

fn connect_new(
    e: &mut Editor,
    from: BlockId,
    side: Side,
    name: &str,
    kind: BlockKind,
) -> Result<BlockId, Box<dyn std::error::Error>> {
    e.start_connect(from, side);
    if let Some(Popup::Connect(form)) = &mut e.popup {
        form.block.name = name.into();
        form.block.kind = kind;
    }
    e.confirm();
    if e.popup.is_some() {
        return Err(format!("popup still open: {:?}", e.message).into());
    }
    e.selected.ok_or_else(|| "no selection".into())
}

fn connect_existing(e: &mut Editor, from: BlockId, side: Side, target: Target) {
    e.start_connect(from, side);
    if let Some(Popup::Connect(form)) = &mut e.popup {
        form.mode = ConnectMode::Existing;
        form.target = Some(target);
    }
    e.confirm();
}

#[test]
fn building_a_context_view_and_undoing_it() -> TestResult {
    let mut e = Editor::new(None);
    assert!(!e.dirty);
    let shop = add_first(&mut e, "Shop")?;
    let user = connect_new(&mut e, shop, Side::Left, "User", BlockKind::Person)?;
    assert_eq!(e.project.blocks.len(), 2);
    assert_eq!(e.project.block(user)?.cell, Cell::new(-1, 0));
    assert!(e.dirty);
    e.undo();
    assert_eq!(e.project.blocks.len(), 1);
    e.redo();
    assert_eq!(e.project.blocks.len(), 2);
    Ok(())
}

#[test]
fn errors_keep_the_popup_open_with_a_message() -> TestResult {
    let mut e = Editor::new(None);
    let shop = add_first(&mut e, "Shop")?;
    e.start_connect(shop, Side::Right);
    if let Some(Popup::Connect(form)) = &mut e.popup {
        form.block.name = "shop".into();
    }
    e.confirm();
    assert!(matches!(e.popup, Some(Popup::Connect(_))));
    assert_eq!(
        e.message.as_deref(),
        Some("\"shop\" already exists in this diagram")
    );
    assert_eq!(e.project.blocks.len(), 1);
    assert!(!e.can_redo());
    Ok(())
}

#[test]
fn neighbour_kinds_are_offered_only_in_the_context_view() -> TestResult {
    let mut e = Editor::new(None);
    let shop = add_first(&mut e, "Shop")?;
    assert!(e.kinds().contains(&BlockKind::Person));
    e.open_diagram(Some(shop));
    assert!(!e.kinds().contains(&BlockKind::Person));
    assert_eq!(e.kinds().len(), 6);
    Ok(())
}

#[test]
fn conflicts_ask_and_keep_leaves_the_box() -> TestResult {
    let mut e = Editor::new(None);
    let a = add_first(&mut e, "A")?;
    let b = connect_new(&mut e, a, Side::Right, "B", BlockKind::Component)?;
    let c = connect_new(&mut e, b, Side::Right, "C", BlockKind::Component)?;
    connect_existing(&mut e, c, Side::Top, Target::Block(b));
    assert!(matches!(e.popup, Some(Popup::Conflict { .. })));
    e.keep_in_place();
    assert!(e.popup.is_none());
    assert_eq!(e.project.block(b)?.cell, Cell::new(1, 0));
    assert_eq!(e.project.relations.len(), 3);
    // The same again, but move.
    e.undo();
    connect_existing(&mut e, c, Side::Top, Target::Block(b));
    e.confirm();
    assert_eq!(e.project.block(b)?.cell, Cell::new(2, -1));
    Ok(())
}

#[test]
fn drilling_down_and_attaching_the_inherited_end() -> TestResult {
    let mut e = Editor::new(None);
    let shop = add_first(&mut e, "Shop")?;
    connect_new(&mut e, shop, Side::Left, "User", BlockKind::Person)?;
    e.open_diagram(Some(shop));
    assert_eq!(e.breadcrumb().len(), 2);
    assert_eq!(e.dangling_count(), 1);
    let ui = add_first(&mut e, "UI")?;
    let candidates = e.connect_candidates(ui, "");
    let dangling = candidates
        .iter()
        .find(|(t, _)| matches!(t, Target::Dangling(..)))
        .ok_or("dangling candidate")?;
    assert_eq!(dangling.1, "User (outside)");
    connect_existing(&mut e, ui, Side::Left, dangling.0);
    assert_eq!(e.dangling_count(), 0);
    e.go_up();
    assert_eq!(e.diagram, None);
    assert_eq!(e.selected, Some(shop));
    Ok(())
}

#[test]
fn clicking_the_open_end_connects_it() -> TestResult {
    let mut e = Editor::new(None);
    let shop = add_first(&mut e, "Shop")?;
    connect_new(&mut e, shop, Side::Left, "User", BlockKind::Person)?;
    e.open_diagram(Some(shop));
    let ui = add_first(&mut e, "UI")?;
    let layout = e.layout().clone();
    let tip = layout
        .lines
        .iter()
        .find_map(|l| l.tip)
        .ok_or("dangling marker")?;
    let Hit::OpenEnd(end) = hit::hit(&e.project, &layout, tip, 1.0) else {
        return Err("marker not hit".into());
    };
    assert!(matches!(end, OpenEnd::Dangling(..)));
    e.start_open_end(end);
    if let Some(Popup::OpenEnd { target, .. }) = &mut e.popup {
        *target = Some(ui);
    }
    e.confirm();
    assert!(e.popup.is_none());
    assert_eq!(e.dangling_count(), 0);
    Ok(())
}

#[test]
fn stubs_can_be_connected_from_another_box() -> TestResult {
    let mut e = Editor::new(None);
    let a = add_first(&mut e, "A")?;
    let b = connect_new(&mut e, a, Side::Right, "B", BlockKind::Component)?;
    e.start_connect(a, Side::Top);
    if let Some(Popup::Connect(form)) = &mut e.popup {
        form.mode = ConnectMode::Stub;
        form.text = "events".into();
        form.direction = Direction::Out;
    }
    e.confirm();
    let stub = e
        .connect_candidates(b, "events")
        .into_iter()
        .find(|(t, _)| matches!(t, Target::Stub(_)))
        .ok_or("stub candidate")?;
    assert_eq!(stub.1, "A (open end): events");
    connect_existing(&mut e, b, Side::Top, stub.0);
    let r = e
        .project
        .relations
        .values()
        .find(|r| r.text == "events")
        .ok_or("rel")?;
    assert!(!r.a.is_open() && !r.b.is_open());
    assert_eq!(
        r.b.anchors.first().map(|x| (x.block, x.side)),
        Some((b, Side::Top))
    );
    Ok(())
}

#[test]
fn hit_testing_finds_sides_boxes_and_lines() -> TestResult {
    let mut e = Editor::new(None);
    let a = add_first(&mut e, "A")?;
    connect_new(&mut e, a, Side::Right, "B", BlockKind::Component)?;
    let layout = e.layout().clone();
    let r = layout.block(a).ok_or("a")?.rect;
    let c = r.center();
    assert_eq!(hit::hit(&e.project, &layout, c, 1.0), Hit::Block(a));
    assert_eq!(
        hit::hit(
            &e.project,
            &layout,
            Pos::new(r.max.x - 2.0, c.y + 20.0),
            1.0
        ),
        Hit::Side(a, Side::Right)
    );
    assert_eq!(
        hit::hit(&e.project, &layout, Pos::new(c.x, r.min.y - 3.0), 1.0),
        Hit::Side(a, Side::Top)
    );
    let line = layout.lines.first().ok_or("line")?;
    let mid = Pos::new(
        (line.points[0].x + line.points[1].x) / 2.0,
        (line.points[0].y + line.points[1].y) / 2.0,
    );
    assert_eq!(
        hit::hit(&e.project, &layout, mid, 1.0),
        Hit::Line(line.relation)
    );
    assert_eq!(
        hit::hit(&e.project, &layout, Pos::new(-50.0, -50.0), 1.0),
        Hit::Empty
    );
    // Dragging beyond the grid gives the next cell out.
    let right = Pos::new(layout.bounds.max.x + 100.0, c.y);
    assert_eq!(hit::cell_at(&layout, right), Some(Cell::new(2, 0)));
    Ok(())
}

#[test]
fn deleting_a_box_with_content_asks_first() -> TestResult {
    let mut e = Editor::new(None);
    let shop = add_first(&mut e, "Shop")?;
    e.open_diagram(Some(shop));
    add_first(&mut e, "Inner")?;
    e.go_up();
    e.request_delete(shop);
    assert!(matches!(
        e.popup,
        Some(Popup::ConfirmDelete { count: 1, .. })
    ));
    e.confirm();
    assert!(e.project.blocks.is_empty());
    e.undo();
    assert_eq!(e.project.blocks.len(), 2);
    Ok(())
}

#[test]
fn nudging_swaps_with_a_neighbour() -> TestResult {
    let mut e = Editor::new(None);
    let a = add_first(&mut e, "A")?;
    let b = connect_new(&mut e, a, Side::Right, "B", BlockKind::Component)?;
    e.selected = Some(a);
    e.nudge(Side::Right);
    assert_eq!(e.project.block(a)?.cell, Cell::new(1, 0));
    assert_eq!(e.project.block(b)?.cell, Cell::new(0, 0));
    Ok(())
}

#[test]
fn autosave_writes_recovery_and_saving_removes_it() -> TestResult {
    let dir = tempfile::tempdir()?;
    let rec = dir.path().join("recovery");
    let file = dir.path().join("shop.yaml");
    let mut e = Editor::new(Some(rec.clone()));
    add_first(&mut e, "Shop")?;
    e.save_as(&file)?;
    assert!(!e.dirty);
    let shop = e.selected.ok_or("sel")?;
    connect_new(&mut e, shop, Side::Right, "DB", BlockKind::Database)?;
    let now = Instant::now();
    assert!(e.tick(now).is_some());
    assert_eq!(e.tick(now + Duration::from_secs(3)), None);
    assert_eq!(std::fs::read_dir(&rec)?.count(), 1);

    // A restart offers the newer unsaved state.
    let mut again = Editor::open(&file, Some(rec.clone()))?;
    assert!(matches!(again.popup, Some(Popup::Restore(_))));
    again.restore();
    assert_eq!(again.project.blocks.len(), 2);
    assert!(again.dirty);
    assert!(again.save()?);
    assert_eq!(std::fs::read_dir(&rec)?.count(), 0);
    let reopened = Editor::open(&file, Some(rec))?;
    assert!(reopened.popup.is_none());
    assert_eq!(reopened.project.blocks.len(), 2);
    Ok(())
}

#[test]
fn export_all_writes_every_diagram() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut e = Editor::new(None);
    let shop = add_first(&mut e, "Shop")?;
    e.open_diagram(Some(shop));
    add_first(&mut e, "Orders/Billing")?;
    assert_eq!(e.export_all(dir.path(), DocFormat::Markdown)?, 2);
    let mut names: Vec<String> = std::fs::read_dir(dir.path())?
        .filter_map(|f| f.ok())
        .map(|f| f.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "context - Shop.md",
            "context - Shop.png",
            "context - Shop.svg",
            "context.md",
            "context.png",
            "context.svg",
            "index.md"
        ]
    );
    Ok(())
}

#[test]
fn tag_colours_change_with_undo() -> TestResult {
    let mut e = Editor::new(None);
    e.start_add_block();
    if let Some(Popup::AddBlock(form)) = &mut e.popup {
        form.name = "Shop".into();
        form.tag = "core".into();
    }
    e.confirm();
    let tag = e.project.tag_by_name("core").ok_or("tag")?;
    let before = e.project.tags[&tag].color;
    let pick = whiteboxed::model::PALETTE[3];
    e.set_tag_color(tag, pick);
    assert_eq!(e.project.tags[&tag].color, pick);
    let fill = e.layout().blocks.first().ok_or("block")?.fill;
    assert_eq!(fill, pick);
    e.undo();
    assert_eq!(e.project.tags[&tag].color, before);
    assert_eq!(e.tag_suggestions("c"), vec!["core".to_owned()]);
    Ok(())
}

#[cfg(unix)]
#[test]
fn the_recovery_directory_is_private() -> TestResult {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir()?;
    let rec = dir.path().join("recovery");
    let mut e = Editor::new(Some(rec.clone()));
    add_first(&mut e, "Shop")?;
    let now = Instant::now();
    e.tick(now + Duration::from_secs(3));
    let mode = std::fs::metadata(&rec)?.permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
    Ok(())
}
