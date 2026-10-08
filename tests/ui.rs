use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use whiteboxed::editor::{Editor, Popup};
use whiteboxed::geom::Pos;
use whiteboxed::model::{BlockId, BlockKind, BlockSpec, Side};
use whiteboxed::ui::App;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn harness(editor: Editor) -> Harness<'static, App> {
    Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), App::new(editor, None))
}

fn press(h: &mut Harness<'_, App>, pos: Pos2, button: PointerButton) {
    h.event(Event::PointerMoved(pos));
    h.step();
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.step();
    }
}

fn type_and_enter(h: &mut Harness<'_, App>, text: &str) {
    h.event(Event::Text(text.into()));
    h.step();
    h.key_press(Key::Enter);
    h.run();
}

fn screen(h: &Harness<'_, App>, p: Pos) -> Result<Pos2, Box<dyn std::error::Error>> {
    Ok(h.state().view().ok_or("no view")?.screen(p))
}

fn one_box() -> Result<(Editor, BlockId), Box<dyn std::error::Error>> {
    let mut e = Editor::new(None);
    let id = e
        .project
        .add_block(None, &BlockSpec::new("Shop", BlockKind::Component))?;
    Ok((e, id))
}

#[test]
fn empty_canvas_offers_add_box() -> TestResult {
    let mut h = harness(Editor::new(None));
    h.run();
    h.get_by_label("+ add box").click();
    h.run();
    assert!(matches!(h.state().editor.popup, Some(Popup::AddBlock(_))));
    type_and_enter(&mut h, "Shop");
    let names: Vec<_> = h
        .state()
        .editor
        .project
        .blocks
        .values()
        .map(|b| b.name.clone())
        .collect();
    assert_eq!(names, vec!["Shop"]);
    assert!(h.state().editor.popup.is_none());
    Ok(())
}

#[test]
fn clicking_a_border_connects_a_new_box() -> TestResult {
    let (e, shop) = one_box()?;
    let mut h = harness(e);
    h.run();
    let r = h
        .state_mut()
        .editor
        .layout()
        .block(shop)
        .ok_or("shop")?
        .rect;
    let edge = screen(&h, Pos::new(r.max.x - 1.0, r.center().y))?;
    press(&mut h, edge, PointerButton::Primary);
    h.run();
    match &h.state().editor.popup {
        Some(Popup::Connect(form)) => assert_eq!((form.from, form.side), (shop, Side::Right)),
        other => return Err(format!("expected connect popup, got {other:?}").into()),
    }
    type_and_enter(&mut h, "Orders");
    let p = &h.state().editor.project;
    assert_eq!(p.blocks.len(), 2);
    assert_eq!(p.relations.len(), 1);
    Ok(())
}

#[test]
fn right_click_menu_deletes_a_box() -> TestResult {
    let (e, shop) = one_box()?;
    let mut h = harness(e);
    h.run();
    let c = h
        .state_mut()
        .editor
        .layout()
        .block(shop)
        .ok_or("shop")?
        .rect
        .center();
    let at = screen(&h, c)?;
    press(&mut h, at, PointerButton::Secondary);
    h.run();
    h.get_by_label("Delete").click();
    h.run();
    assert!(h.state().editor.project.blocks.is_empty());
    Ok(())
}

#[test]
fn keyboard_moves_the_selected_box_and_undo_restores() -> TestResult {
    let (e, shop) = one_box()?;
    let mut h = harness(e);
    h.run();
    let c = h
        .state_mut()
        .editor
        .layout()
        .block(shop)
        .ok_or("shop")?
        .rect
        .center();
    let at = screen(&h, c)?;
    press(&mut h, at, PointerButton::Primary);
    h.run();
    assert_eq!(h.state().editor.selected, Some(shop));
    h.key_press(Key::ArrowDown);
    h.run();
    assert_eq!(h.state().editor.project.block(shop)?.cell.row, 1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(h.state().editor.project.block(shop)?.cell.row, 0);
    Ok(())
}

#[test]
fn breadcrumb_navigates_back_to_the_context() -> TestResult {
    let (mut e, shop) = one_box()?;
    e.open_diagram(Some(shop));
    let mut h = harness(e);
    h.run();
    // Both the breadcrumb and the structure tree offer "Context".
    for node in h.query_all_by_label("Context").take(1) {
        node.click();
    }
    h.run();
    assert_eq!(h.state().editor.diagram, None);
    Ok(())
}

#[test]
fn a_popup_opens_at_each_new_click() -> TestResult {
    let (mut e, shop) = one_box()?;
    let db = e
        .project
        .add_block(None, &BlockSpec::new("DB", BlockKind::Database))?;
    e.project
        .move_block(db, whiteboxed::model::Cell::new(2, 2))?;
    let mut h = harness(e);
    h.run();
    let name_field_at = |h: &mut Harness<'_, App>,
                         block: BlockId,
                         side_x: fn(&whiteboxed::geom::Rect) -> f32|
     -> Result<Pos2, Box<dyn std::error::Error>> {
        let r = h
            .state_mut()
            .editor
            .layout()
            .block(block)
            .ok_or("block")?
            .rect;
        let at = screen(h, Pos::new(side_x(&r), r.center().y))?;
        press(h, at, PointerButton::Primary);
        h.run();
        let pos = h.get_by_label("Name").rect().center();
        h.key_press(Key::Escape);
        h.run();
        Ok(pos)
    };
    let first = name_field_at(&mut h, shop, |r| r.max.x - 1.0)?;
    let second = name_field_at(&mut h, db, |r| r.min.x + 1.0)?;
    assert!(
        // egui keeps the popup on screen, so it moves less than the click did.
        second.x > first.x + 100.0 && second.y > first.y + 50.0,
        "popup stayed at {first:?}, now {second:?}"
    );
    Ok(())
}

fn button<'h>(h: &'h Harness<'_, App>, label: &'h str) -> egui_kittest::Node<'h> {
    h.get_by_role_and_label(egui::accesskit::Role::Button, label)
}

fn block_center(h: &mut Harness<'_, App>, b: BlockId) -> Result<Pos2, Box<dyn std::error::Error>> {
    let c = h
        .state_mut()
        .editor
        .layout()
        .block(b)
        .ok_or("block")?
        .rect
        .center();
    screen(h, c)
}

fn block_edge(
    h: &mut Harness<'_, App>,
    b: BlockId,
    side: Side,
) -> Result<Pos2, Box<dyn std::error::Error>> {
    let r = h.state_mut().editor.layout().block(b).ok_or("block")?.rect;
    let c = r.center();
    let p = match side {
        Side::Top => Pos::new(c.x, r.min.y + 1.0),
        Side::Right => Pos::new(r.max.x - 1.0, c.y),
        Side::Bottom => Pos::new(c.x, r.max.y - 1.0),
        Side::Left => Pos::new(r.min.x + 1.0, c.y),
    };
    screen(h, p)
}

#[test]
fn double_click_opens_the_whitebox() -> TestResult {
    let (e, shop) = one_box()?;
    let mut h = harness(e);
    h.run();
    let at = block_center(&mut h, shop)?;
    // kittest advances time by 0.25 s per step; allow for that between two clicks.
    h.ctx
        .options_mut(|o| o.input_options.max_double_click_delay = 1.0);
    press(&mut h, at, PointerButton::Primary);
    press(&mut h, at, PointerButton::Primary);
    h.run();
    assert_eq!(h.state().editor.diagram, Some(shop));
    assert!(h.get_by_label("+ add box").rect().width() > 0.0);
    Ok(())
}

#[test]
fn connecting_into_a_conflict_offers_keep() -> TestResult {
    let mut e = Editor::new(None);
    let p = &mut e.project;
    let a = p.add_block(None, &BlockSpec::new("A", BlockKind::Component))?;
    let (b, _) = p.connect_new(
        a,
        Side::Right,
        &BlockSpec::new("B", BlockKind::Component),
        whiteboxed::model::Direction::Out,
        "",
    )?;
    let (c, _) = p.connect_new(
        b,
        Side::Right,
        &BlockSpec::new("C", BlockKind::Component),
        whiteboxed::model::Direction::Out,
        "",
    )?;
    let mut h = harness(e);
    h.run();
    let edge = block_edge(&mut h, c, Side::Top)?;
    press(&mut h, edge, PointerButton::Primary);
    h.run();
    button(&h, "Existing").click();
    h.run();
    type_and_enter(&mut h, "B");
    assert!(matches!(
        h.state().editor.popup,
        Some(Popup::Conflict { .. })
    ));
    button(&h, "Keep B, route around").click();
    h.run();
    let ed = &h.state().editor;
    assert!(ed.popup.is_none());
    assert_eq!(ed.project.relations.len(), 3);
    assert_eq!(
        ed.project.block(b)?.cell,
        whiteboxed::model::Cell::new(1, 0)
    );
    Ok(())
}

#[test]
fn the_stub_mode_creates_an_open_end() -> TestResult {
    let (e, shop) = one_box()?;
    let mut h = harness(e);
    h.run();
    let edge = block_edge(&mut h, shop, Side::Bottom)?;
    press(&mut h, edge, PointerButton::Primary);
    h.run();
    button(&h, "Stub").click();
    h.run();
    button(&h, "in").click();
    h.run();
    button(&h, "Connect").click();
    h.run();
    let rel = h
        .state()
        .editor
        .project
        .relations
        .values()
        .next()
        .ok_or("stub")?;
    assert!(rel.b.is_open());
    assert_eq!(rel.direction, whiteboxed::model::Direction::In);
    Ok(())
}

#[test]
fn clicking_a_dangling_marker_attaches_it() -> TestResult {
    let (mut e, shop) = one_box()?;
    e.project.connect_new(
        shop,
        Side::Left,
        &BlockSpec::new("User", BlockKind::Person),
        whiteboxed::model::Direction::Out,
        "uses",
    )?;
    e.project
        .add_block(Some(shop), &BlockSpec::new("UI", BlockKind::Ui))?;
    e.open_diagram(Some(shop));
    let mut h = harness(e);
    h.run();
    assert_eq!(h.state().editor.dangling_count(), 1);
    let tip = h
        .state_mut()
        .editor
        .layout()
        .lines
        .iter()
        .find_map(|l| l.tip)
        .ok_or("marker")?;
    let at = screen(&h, tip)?;
    press(&mut h, at, PointerButton::Primary);
    h.run();
    assert!(matches!(
        h.state().editor.popup,
        Some(Popup::OpenEnd { .. })
    ));
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(h.state().editor.dangling_count(), 0);
    Ok(())
}

#[test]
fn pick_in_diagram_connects_the_clicked_box() -> TestResult {
    let (mut e, shop) = one_box()?;
    let db = e
        .project
        .add_block(None, &BlockSpec::new("DB", BlockKind::Database))?;
    let mut h = harness(e);
    h.run();
    let edge = block_edge(&mut h, shop, Side::Right)?;
    press(&mut h, edge, PointerButton::Primary);
    h.run();
    button(&h, "Existing").click();
    h.run();
    button(&h, "Pick in diagram").click();
    h.run();
    assert!(h.state().is_picking());
    let at = block_center(&mut h, db)?;
    press(&mut h, at, PointerButton::Primary);
    h.run();
    let ed = &h.state().editor;
    assert!(!h.state().is_picking());
    let rel = ed.project.relations.values().next().ok_or("relation")?;
    assert_eq!(rel.b.anchors.first().map(|a| a.block), Some(db));
    Ok(())
}

#[test]
fn delete_key_asks_before_removing_a_box_with_content() -> TestResult {
    let (mut e, shop) = one_box()?;
    e.project
        .add_block(Some(shop), &BlockSpec::new("Inner", BlockKind::Component))?;
    let mut h = harness(e);
    h.run();
    let at = block_center(&mut h, shop)?;
    press(&mut h, at, PointerButton::Primary);
    h.run();
    h.key_press(Key::Delete);
    h.run();
    assert!(matches!(
        h.state().editor.popup,
        Some(Popup::ConfirmDelete { .. })
    ));
    button(&h, "Delete").click();
    h.run();
    assert!(h.state().editor.project.blocks.is_empty());
    Ok(())
}

#[test]
fn closing_with_unsaved_changes_asks_first() -> TestResult {
    let mut h = harness(Editor::new(None));
    h.run();
    h.get_by_label("+ add box").click();
    h.run();
    type_and_enter(&mut h, "Shop");
    assert!(h.state().editor.dirty);
    h.input_mut()
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .events
        .push(egui::ViewportEvent::Close);
    h.run();
    button(&h, "Don't save").click();
    h.step();
    let closes = h
        .output()
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .is_some_and(|v| v.commands.contains(&egui::ViewportCommand::Close));
    assert!(closes, "the window should close after \"Don't save\"");
    Ok(())
}
