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
