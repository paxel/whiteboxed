use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use whiteboxed::editor::{Editor, Popup};
use whiteboxed::geom::Pos;
use whiteboxed::model::{BlockId, BlockKind, BlockSpec, Cell, Side};
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
    assert!(matches!(h.state().editor.popup, Some(Popup::AddBlock(..))));
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
    button(&h, "Quit without saving").click();
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
            listen: whiteboxed::mcp::listen::Listen::Local,
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
    for step in [
        "1. Open a terminal",
        "2. Paste this command",
        "3. Start claude",
    ] {
        assert!(h.query_by_label_contains(step).is_some(), "{step}");
    }
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

#[test]
fn an_ai_export_request_is_answered_in_the_window() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut e = Editor::new(None);
    e.ai_export_request = Some(dir.path().to_path_buf());
    let mut h = harness(e);
    h.run();
    button(&h, "Don't allow").click();
    h.run();
    assert!(h.state().editor.ai_export_request.is_none());
    assert!(h.state().editor.ai_export_roots.is_empty());
    h.state_mut().editor.ai_export_request = Some(dir.path().to_path_buf());
    h.run();
    button(&h, "Allow for this session").click();
    h.run();
    let allowed = std::fs::canonicalize(dir.path())?;
    assert_eq!(h.state().editor.ai_export_roots, vec![allowed]);
    Ok(())
}

fn request_close(h: &mut Harness<'_, App>) {
    h.input_mut()
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .events
        .push(egui::ViewportEvent::Close);
    h.run();
}

fn closes(h: &Harness<'_, App>) -> bool {
    h.output()
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .is_some_and(|v| v.commands.contains(&egui::ViewportCommand::Close))
}

#[test]
fn escape_keeps_editing_and_enter_saves_before_quitting() -> TestResult {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("shop.yaml");
    let (mut e, _) = one_box()?;
    e.save_as(&file)?;
    e.project
        .add_block(None, &BlockSpec::new("Billing", BlockKind::Component))?;
    e.dirty = true;
    let mut h = harness(e);
    h.run();
    request_close(&mut h);
    // The project is named after its system box, not after the file.
    assert!(
        h.query_by_label("Save changes to \u{201c}Shop\u{201d}?")
            .is_some()
    );
    h.key_press(Key::Escape);
    h.run();
    assert!(
        h.query_by_label("Keep editing").is_none(),
        "the dialog is gone"
    );
    assert!(!closes(&h));
    assert!(!h.state().is_closing());
    request_close(&mut h);
    h.key_press(Key::Enter);
    h.run();
    assert!(h.state().is_closing(), "Enter saves and quits");
    assert!(!h.state().editor.dirty);
    assert!(std::fs::read_to_string(&file)?.contains("Billing"));
    Ok(())
}

#[test]
fn project_settings_change_the_line_style() -> TestResult {
    use whiteboxed::model::LineStyle;
    let (e, _) = one_box()?;
    let mut h = harness(e);
    h.run();
    h.get_by_label("File").click();
    h.run();
    h.get_by_label("Project settings\u{2026}").click();
    h.run();
    h.get_by_label("curved").click();
    h.run();
    assert_eq!(h.state().editor.project.line_style, LineStyle::Curved);
    h.get_by_label("Use as my default for new projects").click();
    h.run();
    assert_eq!(h.state().settings.prefs.line_style, LineStyle::Curved);
    Ok(())
}

#[test]
fn right_click_on_empty_space_adds_a_box_in_that_cell() -> TestResult {
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
    // One cell to the right of the shop, below it.
    let at = screen(&h, Pos::new(c.x, c.y + 260.0))?;
    press(&mut h, at, PointerButton::Secondary);
    h.run();
    h.get_by_label("Add box here").click();
    h.run();
    assert!(matches!(
        h.state().editor.popup,
        Some(Popup::AddBlock(_, Some(cell))) if cell == Cell::new(0, 1)
    ));
    type_and_enter(&mut h, "Billing");
    let billing = h
        .state()
        .editor
        .project
        .blocks
        .values()
        .find(|b| b.name == "Billing")
        .map(|b| b.cell);
    assert_eq!(billing, Some(Cell::new(0, 1)));
    Ok(())
}

#[test]
fn the_menu_bar_adds_a_box_in_the_next_free_cell() -> TestResult {
    let (e, _) = one_box()?;
    let mut h = harness(e);
    h.run();
    h.get_by_label("+ Box").click();
    h.run();
    assert!(matches!(
        h.state().editor.popup,
        Some(Popup::AddBlock(_, None))
    ));
    type_and_enter(&mut h, "Billing");
    let billing = h
        .state()
        .editor
        .project
        .blocks
        .values()
        .find(|b| b.name == "Billing")
        .map(|b| b.cell);
    assert_eq!(billing, Some(Cell::new(1, 0)));
    Ok(())
}

fn zoom(h: &Harness<'_, App>) -> Result<f32, Box<dyn std::error::Error>> {
    Ok(h.state().view().ok_or("no view")?.zoom)
}

fn add_box_at(
    h: &mut Harness<'_, App>,
    name: &str,
    cell: Cell,
) -> Result<BlockId, Box<dyn std::error::Error>> {
    let e = &mut h.state_mut().editor;
    e.start_add_block_at(cell);
    if let Some(Popup::AddBlock(form, _)) = &mut e.popup {
        form.name = name.into();
    }
    e.confirm();
    h.run();
    h.state()
        .editor
        .project
        .blocks
        .iter()
        .find(|(_, b)| b.name == name)
        .map(|(id, _)| *id)
        .ok_or_else(|| "not added".into())
}

#[test]
fn zoom_buttons_and_keys_change_the_zoom() -> TestResult {
    let (e, _) = one_box()?;
    let mut h = harness(e);
    h.run();
    let fitted = zoom(&h)?;
    h.get_by_label("+").click();
    h.run();
    assert!((zoom(&h)? - fitted * 1.25).abs() < 0.001);
    let level = format!("{:.0} %", zoom(&h)? * 100.0);
    h.get_by_label(&level).click();
    h.run();
    assert_eq!(zoom(&h)?, 1.0);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Equals);
    h.run();
    assert_eq!(zoom(&h)?, 1.25);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Minus);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Minus);
    h.run();
    assert!((zoom(&h)? - 0.8).abs() < 0.001);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num0);
    h.run();
    assert_eq!(zoom(&h)?, 1.0);
    h.get_by_label("Fit").click();
    h.run();
    assert_eq!(zoom(&h)?, fitted);
    Ok(())
}

#[test]
fn editing_keeps_the_zoom_and_the_picture_in_place() -> TestResult {
    let (e, shop) = one_box()?;
    let mut h = harness(e);
    h.run();
    // Zoomed out far enough that the new box needs no scrolling.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Minus);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Minus);
    h.run();
    let z = zoom(&h)?;
    let shop_at = |h: &mut Harness<'_, App>| -> Result<Pos2, Box<dyn std::error::Error>> {
        let r = h
            .state_mut()
            .editor
            .layout()
            .block(shop)
            .ok_or("shop")?
            .rect;
        screen(h, r.min)
    };
    let before = shop_at(&mut h)?;
    // A box in front of the shop shifts the whole grid; the shop stays where it was.
    add_box_at(&mut h, "Left", Cell::new(-1, 0))?;
    assert_eq!(zoom(&h)?, z);
    let after = shop_at(&mut h)?;
    assert!((after - before).length() < 0.5, "{before:?} -> {after:?}");
    Ok(())
}

#[test]
fn a_new_box_outside_the_view_is_scrolled_into_it() -> TestResult {
    let (e, _) = one_box()?;
    let mut h = harness(e);
    h.run();
    for _ in 0..8 {
        h.key_press_modifiers(Modifiers::COMMAND, Key::Equals);
    }
    h.run();
    let z = zoom(&h)?;
    let far = add_box_at(&mut h, "Far", Cell::new(3, 2))?;
    assert_eq!(zoom(&h)?, z, "the zoom stays");
    let r = h.state_mut().editor.layout().block(far).ok_or("far")?.rect;
    let (min, max) = (screen(&h, r.min)?, screen(&h, r.max)?);
    let canvas = h.state().canvas_rect();
    assert!(
        canvas.contains(min) && canvas.contains(max),
        "{min:?}..{max:?} not in {canvas:?}"
    );
    Ok(())
}

#[test]
fn project_settings_name_the_project() -> TestResult {
    let (e, _) = one_box()?;
    let mut h = harness(e);
    h.run();
    h.get_by_label("File").click();
    h.run();
    h.get_by_label("Project settings\u{2026}").click();
    h.run();
    h.get_by_role(egui::accesskit::Role::TextInput).click();
    h.run();
    h.event(Event::Text("Sanshain".into()));
    h.run();
    assert_eq!(
        h.state().editor.project.name,
        "",
        "typing alone does not change it"
    );
    h.key_press(Key::Tab);
    h.run();
    assert_eq!(h.state().editor.project.name, "Sanshain");
    assert_eq!(h.state().editor.title(), "whiteboxed \u{2013} Sanshain *");
    Ok(())
}

#[test]
fn the_relation_dialog_offers_no_direction() -> TestResult {
    let (mut e, shop) = one_box()?;
    e.project.connect_new(
        shop,
        Side::Right,
        &BlockSpec::new("Billing", BlockKind::Component),
        whiteboxed::model::Direction::Out,
        "",
    )?;
    let rel = *e.project.relations.keys().next().ok_or("rel")?;
    let mut h = harness(e);
    h.run();
    h.state_mut().editor.start_edit_relation(rel);
    h.run();
    h.get_by_label("none").click();
    h.run();
    h.get_by_label("Save").click();
    h.run();
    assert_eq!(
        h.state().editor.project.relation(rel)?.direction,
        whiteboxed::model::Direction::Undirected
    );
    Ok(())
}

#[test]
fn the_add_box_dialog_makes_a_band() -> TestResult {
    let (e, _) = one_box()?;
    let mut h = harness(e);
    h.run();
    h.get_by_label("+ Box").click();
    h.run();
    h.event(Event::Text("Logging".into()));
    h.run();
    h.get_by_label("Cross-cutting band").click();
    h.run();
    h.get_by_label("Add").click();
    h.run();
    let band = h
        .state()
        .editor
        .project
        .blocks
        .values()
        .find(|b| b.name == "Logging")
        .map(|b| b.band);
    assert_eq!(band, Some(true));
    Ok(())
}

#[test]
fn deleting_one_branch_of_a_fanned_out_line_keeps_the_other() -> TestResult {
    use whiteboxed::model::{Direction, End};
    let mut e = Editor::new(None);
    let user = e
        .project
        .add_block(None, &BlockSpec::new("Customer", BlockKind::Person))?;
    let (shop, rel) = e.project.connect_new(
        user,
        Side::Right,
        &BlockSpec::new("Shop", BlockKind::Component),
        Direction::Out,
        "orders",
    )?;
    let ui = e
        .project
        .add_block(Some(shop), &BlockSpec::new("UI", BlockKind::Component))?;
    let (api, _) = e.project.connect_new(
        ui,
        Side::Bottom,
        &BlockSpec::new("API", BlockKind::Component),
        Direction::Out,
        "",
    )?;
    e.project.attach(rel, End::B, shop, ui, Side::Left)?;
    e.project.attach(rel, End::B, shop, api, Side::Left)?;
    e.open_diagram(Some(shop));
    let mut h = harness(e);
    h.run();
    let branch = h
        .state_mut()
        .editor
        .layout()
        .lines
        .iter()
        .find(|l| l.landing == Some(api))
        .map(|l| l.points.clone())
        .ok_or("branch to API")?;
    // The last stretch, into API, belongs to this branch alone.
    let n = branch.len();
    let mid = Pos::new(
        (branch[n - 2].x + branch[n - 1].x) / 2.0,
        (branch[n - 2].y + branch[n - 1].y) / 2.0,
    );
    let at = screen(&h, mid)?;
    press(&mut h, at, PointerButton::Secondary);
    h.run();
    h.get_by_label("Delete").click();
    h.run();
    let p = &h.state().editor.project;
    let landed: Vec<BlockId> = p
        .landings(&p.relation(rel)?.b, shop)
        .iter()
        .map(|a| a.block)
        .collect();
    assert_eq!(landed, vec![ui]);
    Ok(())
}

#[test]
fn the_export_dialog_writes_the_files_and_remembers_the_choice() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (e, _) = one_box()?;
    let mut h = harness(e);
    h.run();
    h.get_by_label("File").click();
    h.run();
    h.get_by_label("Export\u{2026}").click();
    h.run();
    assert!(h.state().export.open);
    let target = dir.path().join("docs");
    h.state_mut().export.set_folder(target.clone());
    h.get_by_label("Markdown").click();
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Export")
        .click();
    h.run();
    assert!(!h.state().export.open);
    assert!(target.join("context.svg").exists());
    assert!(target.join("context.png").exists());
    assert!(target.join("index.md").exists());
    let remembered = h
        .state()
        .editor
        .project
        .export
        .clone()
        .ok_or("not remembered")?;
    assert_eq!(remembered.text, Some(whiteboxed::doc::DocFormat::Markdown));
    assert_eq!(remembered.folder, target.to_string_lossy());
    Ok(())
}

#[test]
fn the_ai_dialog_chooses_who_may_connect() -> TestResult {
    use whiteboxed::mcp::listen::Listen;
    let dir = ai_dir(0)?;
    let mut h = ai_harness(Editor::new(None), &dir);
    h.run();
    let ctx = h.ctx.clone();
    h.state_mut().ai.start(&ctx);
    h.run();
    assert!(h.state().ai.is_on());
    if cfg!(target_os = "linux") && !std::path::Path::new("/sys/class/net/docker0").exists() {
        h.get_by_label("Docker containers on this computer").click();
        h.run();
        assert!(h.query_by_label_contains("No Docker bridge").is_some());
        assert!(!h.state().ai.is_on());
    }
    // The port comes first, the address below it.
    h.get_all_by_role(egui::accesskit::Role::TextInput)
        .last()
        .ok_or("address field")?
        .click();
    h.run();
    h.event(Event::Text("127.0.0.1".into()));
    h.run();
    button(&h, "Use this address").click();
    h.run();
    assert!(h.state().ai.is_on());
    assert!(
        h.query_by_label_contains("Only the token keeps them out")
            .is_some()
    );
    let saved = whiteboxed::mcp::settings::load_or_create(dir.path())?;
    assert_eq!(saved.listen, Listen::Custom("127.0.0.1".into()));
    let cmd = h.state_mut().ai.claude_command().ok_or("command")?;
    assert!(cmd.contains("http://127.0.0.1:"), "{cmd}");
    Ok(())
}

#[test]
fn the_details_panel_explains_readability() -> TestResult {
    let mut e = Editor::new(None);
    for i in 0..10 {
        e.project.add_block(
            None,
            &BlockSpec::new(&format!("B{i}"), BlockKind::Component),
        )?;
    }
    let mut h = harness(e);
    h.run();
    assert!(h.query_by_label("Readability").is_some());
    assert!(h.query_by_label("hard to read").is_some());
    assert!(h.query_by_label("10 boxes").is_some());
    Ok(())
}

#[test]
fn right_click_moves_a_box_up_a_level() -> TestResult {
    let (mut e, shop) = one_box()?;
    let inner = e
        .project
        .add_block(Some(shop), &BlockSpec::new("Inner", BlockKind::Component))?;
    e.open_diagram(Some(shop));
    let mut h = harness(e);
    h.run();
    let c = h
        .state_mut()
        .editor
        .layout()
        .block(inner)
        .ok_or("inner")?
        .rect
        .center();
    let at = screen(&h, c)?;
    press(&mut h, at, PointerButton::Secondary);
    h.run();
    h.get_by_label("Move up a level").click();
    h.run();
    assert_eq!(h.state().editor.project.block(inner)?.parent, None);
    Ok(())
}

#[test]
fn move_into_picks_the_target_with_a_click() -> TestResult {
    let (mut e, shop) = one_box()?;
    let cache = e
        .project
        .add_block(None, &BlockSpec::new("Cache", BlockKind::Cache))?;
    let mut h = harness(e);
    h.run();
    let center = |h: &mut Harness<'_, App>, b| -> Result<Pos2, Box<dyn std::error::Error>> {
        let c = h
            .state_mut()
            .editor
            .layout()
            .block(b)
            .ok_or("box")?
            .rect
            .center();
        screen(h, c)
    };
    let at = center(&mut h, cache)?;
    press(&mut h, at, PointerButton::Secondary);
    h.run();
    h.get_by_label("Move into\u{2026}").click();
    h.run();
    assert!(
        h.query_by_label("Click the box to move it into. Esc cancels.")
            .is_some()
    );
    let target = center(&mut h, shop)?;
    press(&mut h, target, PointerButton::Primary);
    h.run();
    assert_eq!(h.state().editor.project.block(cache)?.parent, Some(shop));
    Ok(())
}

#[test]
fn ctrl_click_and_right_click_group_boxes() -> TestResult {
    let (mut e, shop) = one_box()?;
    let cache = e
        .project
        .add_block(None, &BlockSpec::new("Cache", BlockKind::Cache))?;
    let mut h = harness(e);
    h.run();
    let center = |h: &mut Harness<'_, App>, b| -> Result<Pos2, Box<dyn std::error::Error>> {
        let c = h
            .state_mut()
            .editor
            .layout()
            .block(b)
            .ok_or("box")?
            .rect
            .center();
        screen(h, c)
    };
    let (at_shop, at_cache) = (center(&mut h, shop)?, center(&mut h, cache)?);
    press(&mut h, at_shop, PointerButton::Primary);
    h.run();
    h.event(Event::PointerMoved(at_cache));
    h.step();
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos: at_cache,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::COMMAND,
        });
        h.step();
    }
    h.run();
    assert_eq!(h.state().editor.selection(), vec![shop, cache]);
    press(&mut h, at_shop, PointerButton::Secondary);
    h.run();
    h.get_by_label("Group 2 boxes into new box\u{2026}").click();
    h.run();
    type_and_enter(&mut h, "Backend");
    let p = &h.state().editor.project;
    let backend = p
        .blocks
        .iter()
        .find(|(_, b)| b.name == "Backend")
        .map(|(id, _)| *id)
        .ok_or("no group box")?;
    assert_eq!(p.block(shop)?.parent, Some(backend));
    assert_eq!(p.block(cache)?.parent, Some(backend));
    Ok(())
}

#[test]
fn right_click_dissolves_a_whitebox_after_asking() -> TestResult {
    let (mut e, shop) = one_box()?;
    e.project.set_responsibility(shop, "Sells.")?;
    let inner = e
        .project
        .add_block(Some(shop), &BlockSpec::new("Inner", BlockKind::Component))?;
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
    h.get_by_label("Dissolve whitebox\u{2026}").click();
    h.run();
    assert!(
        h.query_by_label_contains("removed with its responsibility")
            .is_some()
    );
    h.get_by_label("Dissolve").click();
    h.run();
    let p = &h.state().editor.project;
    assert!(p.block(shop).is_err());
    assert_eq!(p.block(inner)?.parent, None);
    Ok(())
}
