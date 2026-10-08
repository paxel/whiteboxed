//! Tests that render the real window through wgpu. They need a GPU or a software
//! Vulkan driver, so they only run with `--features render-tests` (CI: lavapipe).
//! The ignored `doc_screenshot_*` tests write the README screenshots.
#![cfg(feature = "render-tests")]

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use whiteboxed::editor::Editor;
use whiteboxed::geom::Pos;
use whiteboxed::model::{
    BlockId, BlockKind, BlockSpec, Direction, End, Placement, Project, Rgb, Side,
};
use whiteboxed::ui::App;

type TestResult = Result<(), Box<dyn std::error::Error>>;
type Image = image::RgbaImage;

fn harness(editor: Editor) -> Harness<'static, App> {
    let h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut App| app.show(ui), App::new(editor, None));
    h.ctx.set_theme(egui::Theme::Light);
    h
}

fn screen(h: &Harness<'_, App>, p: Pos) -> Result<Pos2, Box<dyn std::error::Error>> {
    Ok(h.state().view().ok_or("no view")?.screen(p))
}

fn press(h: &mut Harness<'_, App>, pos: Pos2) {
    h.event(Event::PointerMoved(pos));
    h.step();
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.step();
    }
    h.run();
}

fn pixel(img: &Image, p: Pos2) -> Rgb {
    let [r, g, b, _] = img.get_pixel(p.x as u32, p.y as u32).0;
    Rgb(r, g, b)
}

fn close(a: Rgb, b: Rgb) -> bool {
    let d = |x: u8, y: u8| (i16::from(x) - i16::from(y)).abs() <= 4;
    d(a.0, b.0) && d(a.1, b.1) && d(a.2, b.2)
}

/// The example from the README: a web shop, its context and its whitebox.
fn shop() -> Result<(Project, BlockId), Box<dyn std::error::Error>> {
    let mut p = Project::new();
    let spec = BlockSpec::new;
    let user = p.add_block(None, &spec("Customer", BlockKind::Person))?;
    let (shop, buy) = p.connect_new(
        user,
        Side::Right,
        &spec("Web Shop", BlockKind::Component).tagged("core"),
        Direction::Out,
        "orders via browser",
    )?;
    let (pay, _) = p.connect_new(
        shop,
        Side::Right,
        &spec("Payment Provider", BlockKind::ExternalSystem),
        Direction::Bi,
        "REST",
    )?;
    p.connect_new(
        shop,
        Side::Bottom,
        &spec("Warehouse", BlockKind::ExternalSystem),
        Direction::Out,
        "shipping orders",
    )?;
    p.connect_existing(
        user,
        Side::Top,
        pay,
        Direction::In,
        "receipt",
        Placement::Keep,
    )?;
    let ui = p.add_block(Some(shop), &spec("Storefront", BlockKind::Ui))?;
    let (orders, _) = p.connect_new(
        ui,
        Side::Right,
        &spec("Orders", BlockKind::Component).tagged("core"),
        Direction::Out,
        "place order",
    )?;
    p.connect_new(
        orders,
        Side::Bottom,
        &spec("Order DB", BlockKind::Database),
        Direction::Out,
        "SQL",
    )?;
    p.connect_new(
        orders,
        Side::Right,
        &spec("Events", BlockKind::Queue),
        Direction::Out,
        "publish",
    )?;
    p.connect_new(
        ui,
        Side::Bottom,
        &spec("Session Cache", BlockKind::Cache),
        Direction::Bi,
        "",
    )?;
    p.attach(buy, End::B, shop, ui, Side::Left)?;
    Ok((p, shop))
}

fn editor_with(project: Project) -> Editor {
    let mut e = Editor::new(None);
    e.project = project;
    e
}

#[test]
fn the_canvas_paints_boxes_in_their_tag_colour() -> TestResult {
    let (p, shop) = shop()?;
    let colour = p
        .tag_by_name("core")
        .and_then(|t| p.tags.get(&t))
        .ok_or("tag")?
        .color;
    let mut h = harness(editor_with(p));
    h.run();
    let r = h
        .state_mut()
        .editor
        .layout()
        .block(shop)
        .ok_or("shop")?
        .rect;
    let inside = screen(&h, Pos::new(r.min.x + 12.0, r.min.y + 12.0))?;
    let img = h.render()?;
    let got = pixel(&img, inside);
    assert!(
        close(got, colour),
        "expected {colour} inside the box, got {got}"
    );
    Ok(())
}

#[test]
fn hovering_a_border_highlights_that_side() -> TestResult {
    let (p, shop) = shop()?;
    let mut h = harness(editor_with(p));
    h.run();
    let r = h
        .state_mut()
        .editor
        .layout()
        .block(shop)
        .ok_or("shop")?
        .rect;
    let edge = screen(&h, Pos::new(r.max.x - 1.0, r.min.y + 10.0))?;
    h.hover_at(edge);
    h.run();
    let img = h.render()?;
    let accent = Rgb(0x1e, 0x88, 0xe5);
    let got = pixel(&img, edge);
    assert!(
        close(got, accent),
        "expected the accent on the hovered side, got {got}"
    );
    Ok(())
}

#[test]
fn the_whitebox_marks_unassigned_interfaces() -> TestResult {
    let (p, shop) = shop()?;
    let mut e = editor_with(p);
    e.open_diagram(Some(shop));
    let mut h = harness(e);
    h.run();
    let tip = h
        .state_mut()
        .editor
        .layout()
        .lines
        .iter()
        .find(|l| l.kind == whiteboxed::layout::LineKind::Dangling)
        .and_then(|l| l.tip)
        .ok_or("dangling marker")?;
    let at = screen(&h, Pos::new(tip.x, tip.y + 4.0))?;
    let img = h.render()?;
    let got = pixel(&img, at);
    assert!(
        close(got, whiteboxed::scene::WARN),
        "expected the warning marker, got {got}"
    );
    Ok(())
}

// ----- README screenshots -----

fn save(h: &mut Harness<'_, App>, name: &str) -> TestResult {
    let img = h.render()?;
    std::fs::create_dir_all("docs/screenshots")?;
    img.save(format!("docs/screenshots/{name}.png"))?;
    Ok(())
}

#[test]
#[ignore = "writes docs/screenshots (needs wgpu)"]
fn doc_screenshot_context_view() -> TestResult {
    let (p, _) = shop()?;
    let mut h = harness(editor_with(p));
    h.run();
    save(&mut h, "context")
}

#[test]
#[ignore = "writes docs/screenshots (needs wgpu)"]
fn doc_screenshot_whitebox() -> TestResult {
    let (p, shop) = shop()?;
    let mut e = editor_with(p);
    e.open_diagram(Some(shop));
    let mut h = harness(e);
    h.run();
    save(&mut h, "whitebox")
}

#[test]
#[ignore = "writes docs/screenshots (needs wgpu)"]
fn doc_screenshot_connect_popup() -> TestResult {
    let (p, shop) = shop()?;
    let mut h = harness(editor_with(p));
    h.run();
    let r = h
        .state_mut()
        .editor
        .layout()
        .block(shop)
        .ok_or("shop")?
        .rect;
    let edge = screen(&h, Pos::new(r.center().x, r.min.y + 1.0))?;
    press(&mut h, edge);
    h.event(Event::Text("Product Catalog".into()));
    h.run();
    save(&mut h, "connect")
}

#[test]
#[ignore = "writes docs/screenshots (needs wgpu)"]
fn doc_screenshot_empty_project() -> TestResult {
    let mut h = harness(Editor::new(None));
    h.run();
    save(&mut h, "empty")
}
