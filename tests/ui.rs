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

fn details_field<'h>(h: &'h Harness<'_, App>) -> egui_kittest::Node<'h> {
    h.get_by_role(egui::accesskit::Role::MultilineTextInput)
}

#[test]
fn the_details_panel_edits_responsibility_with_one_undo_step() -> TestResult {
    let (e, shop) = one_box()?;
    let mut h = harness(e);
    h.run();
    let at = block_center(&mut h, shop)?;
    press(&mut h, at, PointerButton::Primary);
    h.run();
    assert!(h.query_by_label("Responsibility").is_some());
    details_field(&h).click();
    h.run();
    h.event(Event::Text("Sells things.".into()));
    h.run();
    // Ctrl+Z inside the field is the field's own undo, never the project's.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(h.state().editor.project.blocks.len(), 1);
    h.event(Event::Text("Sells things.".into()));
    h.run();
    // Leaving the field (a click on empty canvas) writes the text.
    press(&mut h, Pos2::new(600.0, 700.0), PointerButton::Primary);
    h.run();
    assert_eq!(
        h.state().editor.project.block(shop)?.responsibility,
        "Sells things."
    );
    assert!(h.state().editor.dirty);
    // Nothing is selected now: the panel describes the diagram.
    assert!(h.query_by_label("Motivation").is_some());
    button(&h, "Undo").click();
    h.run();
    assert_eq!(h.state().editor.project.block(shop)?.responsibility, "");
    button(&h, "Redo").click();
    h.run();
    assert_eq!(
        h.state().editor.project.block(shop)?.responsibility,
        "Sells things."
    );
    Ok(())
}

#[test]
fn the_motivation_is_kept_when_switching_to_a_box() -> TestResult {
    let (e, shop) = one_box()?;
    let mut h = harness(e);
    h.run();
    details_field(&h).click();
    h.run();
    h.event(Event::Text("Who uses the shop.".into()));
    h.run();
    let at = block_center(&mut h, shop)?;
    press(&mut h, at, PointerButton::Primary);
    h.run();
    assert_eq!(h.state().editor.project.motivation, "Who uses the shop.");
    assert_eq!(h.state().editor.selected, Some(shop));
    Ok(())
}

#[test]
fn the_undo_button_is_disabled_without_history() -> TestResult {
    let (e, _) = one_box()?;
    let mut h = harness(e);
    h.run();
    button(&h, "Undo").click();
    h.run();
    assert!(!h.state().editor.can_undo());
    assert_eq!(h.state().editor.project.blocks.len(), 1);
    Ok(())
}

fn ai_dir(port: u16) -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    whiteboxed::mcp::settings::save(
        dir.path(),
        &whiteboxed::mcp::settings::AiSettings {
            port,
            token: "tok-123".into(),
        },
    )?;
    Ok(dir)
}

fn ai_harness(editor: Editor, dir: &tempfile::TempDir) -> Harness<'static, App> {
    let app = App::new(editor, None).with_ai_dir(Some(dir.path().to_path_buf()));
    Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app)
}

fn ai_call(
    h: &mut Harness<'_, App>,
    tool: &str,
    args: serde_json::Value,
) -> Result<whiteboxed::api::Output, Box<dyn std::error::Error>> {
    let (reply, answer) = tokio::sync::oneshot::channel();
    h.state().ai.sender().send(whiteboxed::mcp::server::Call {
        tool: tool.into(),
        args,
        reply,
    })?;
    h.run();
    Ok(answer.blocking_recv()??)
}

#[test]
fn turning_ai_access_on_shows_how_to_connect() -> TestResult {
    let dir = ai_dir(0)?;
    let mut h = ai_harness(Editor::new(None), &dir);
    h.run();
    h.get_by_label("AI").click();
    h.run();
    h.get_by_label("Allow AI access").click();
    h.run();
    assert!(h.state().ai.is_on());
    assert!(h.query_by_label("AI access: On").is_some());
    let url = h.state().ai.url().ok_or("no url")?;
    let port: u16 = url
        .trim_end_matches("/mcp")
        .rsplit(':')
        .next()
        .ok_or("port")?
        .parse()?;
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_ok());
    let cmd = h.state_mut().ai.claude_command().ok_or("command")?;
    assert!(cmd.starts_with("claude mcp add --transport http whiteboxed http://127.0.0.1:"));
    assert!(cmd.ends_with("--header \"Authorization: Bearer tok-123\""));
    button(&h, "Turn off").click();
    h.run();
    assert!(!h.state().ai.is_on());
    Ok(())
}

#[test]
fn a_taken_port_is_reported_in_the_dialog() -> TestResult {
    let taken = std::net::TcpListener::bind("127.0.0.1:0")?;
    let dir = ai_dir(taken.local_addr()?.port())?;
    let mut h = ai_harness(Editor::new(None), &dir);
    h.run();
    let ctx = h.ctx.clone();
    h.state_mut().ai.start(&ctx);
    h.run();
    assert!(!h.state().ai.is_on());
    assert!(h.query_by_label_contains("cannot be used").is_some());
    Ok(())
}

#[test]
fn ai_changes_show_up_and_the_view_follows() -> TestResult {
    let dir = ai_dir(0)?;
    let mut h = ai_harness(Editor::new(None), &dir);
    h.run();
    ai_call(
        &mut h,
        "add_box",
        serde_json::json!({"name": "Shop", "kind": "component"}),
    )?;
    ai_call(
        &mut h,
        "add_box",
        serde_json::json!({"diagram": "Shop", "name": "Orders", "kind": "component"}),
    )?;
    let e = &h.state().editor;
    assert_eq!(e.project.blocks.len(), 2);
    // Follow AI is on by default: the view went into Shop's whitebox.
    let shop = e
        .project
        .blocks
        .iter()
        .find(|(_, b)| b.name == "Shop")
        .map(|(id, _)| *id);
    assert_eq!(e.diagram, shop);
    assert!(e.selected.is_some());
    // One undo step per call.
    h.state_mut().editor.undo();
    assert_eq!(h.state().editor.project.blocks.len(), 1);

    h.state_mut().ai.follow = false;
    h.state_mut().editor.open_diagram(None);
    ai_call(
        &mut h,
        "add_box",
        serde_json::json!({"diagram": "Shop", "name": "Billing", "kind": "component"}),
    )?;
    assert_eq!(h.state().editor.diagram, None);
    let refused = ai_call(
        &mut h,
        "add_box",
        serde_json::json!({"name": "shop", "kind": "component"}),
    );
    assert!(refused.is_err());
    Ok(())
}
