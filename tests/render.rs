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

/// Whether `color` is drawn within `radius` pixels of `p`. Thin shapes blend with
/// their surroundings at single pixels (anti-aliasing), so look around.
fn drawn_near(img: &Image, p: Pos2, color: Rgb, radius: i32) -> bool {
    (-radius..=radius).any(|dx| {
        (-radius..=radius).any(|dy| close(pixel(img, p + egui::vec2(dx as f32, dy as f32)), color))
    })
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
    let logging = p.add_block(
        Some(shop),
        &spec("Logging and Monitoring", BlockKind::Component).as_band(),
    )?;
    p.set_responsibility(
        logging,
        "Collects logs and metrics of every part of the shop.",
    )?;
    p.set_motivation(
        None,
        "Customers buy through the web shop; payment and shipping are external services.",
    )?;
    p.set_responsibility(
        shop,
        "Sells the catalogue online: browsing, ordering and payment.\n\nOwns orders until they are handed to the warehouse.",
    )?;
    p.set_responsibility(ui, "Renders the shop pages and holds the session.")?;
    p.set_responsibility(orders, "Validates orders and publishes order events.")?;
    p.set_motivation(
        Some(shop),
        "Split by responsibility: presentation, order handling and storage are separate so each can scale on its own.",
    )?;
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
    // The highlight is a 5 px line on the edge; anti-aliasing blends single pixels
    // with the outline, so look for the accent anywhere across the line.
    let found = drawn_near(&img, edge, accent, 4);
    let got = pixel(&img, edge);
    let view = h.state().view().ok_or("no view")?;
    let layout = h.state_mut().editor.layout().clone();
    let under = whiteboxed::hit::hit(
        &h.state().editor.project,
        &layout,
        view.diagram(edge),
        view.zoom,
    );
    assert!(
        found,
        "expected the accent on the hovered side, got {got} at the edge ({under:?}, zoom {})",
        view.zoom
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
    let at = screen(&h, Pos::new(tip.x, tip.y + 2.0))?;
    let img = h.render()?;
    let got = pixel(&img, at);
    assert!(
        drawn_near(&img, at, whiteboxed::scene::WARN, 3),
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
    let (p, shop) = shop()?;
    let mut e = editor_with(p);
    e.selected = Some(shop);
    let mut h = harness(e);
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

#[test]
#[ignore = "writes docs/screenshots (needs wgpu)"]
fn doc_screenshot_ai_access() -> TestResult {
    let dir = tempfile::tempdir()?;
    whiteboxed::mcp::settings::save(
        dir.path(),
        &whiteboxed::mcp::settings::AiSettings {
            port: 7342,
            token: "5e7c2a91d04b4f3c8a1e6b0d9f27c4a85b3e1f60c7d24a9e8b16f05c3d72a4e1".into(),
        },
    )?;
    let (p, shop) = shop()?;
    let mut e = editor_with(p);
    e.last_ai = Some(whiteboxed::editor::AiAction {
        summary: "added Orders next to Storefront".into(),
        diagram: Some(shop),
        block: None,
    });
    let app = App::new(e, None).with_ai_dir(Some(dir.path().to_path_buf()));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.ctx.set_theme(egui::Theme::Light);
    h.run();
    let ctx = h.ctx.clone();
    h.state_mut().ai.start(&ctx);
    h.run();
    save(&mut h, "ai-access")
}

#[test]
#[ignore = "writes docs/screenshots (needs wgpu)"]
fn doc_screenshot_unsaved_changes() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (p, _) = shop()?;
    let mut e = editor_with(p);
    e.save_as(&dir.path().join("Web Shop.yaml"))?;
    e.dirty = true;
    let mut h = harness(e);
    h.run();
    h.input_mut()
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .events
        .push(egui::ViewportEvent::Close);
    h.run();
    save(&mut h, "unsaved-changes")
}

#[test]
#[ignore = "writes docs/screenshots (needs wgpu)"]
fn doc_screenshot_project_settings() -> TestResult {
    let (p, _) = shop()?;
    let mut h = harness(editor_with(p));
    h.run();
    h.state_mut().settings.open = true;
    h.run();
    save(&mut h, "project-settings")
}

#[test]
#[ignore = "writes docs/screenshots (needs wgpu)"]
fn doc_screenshot_export() -> TestResult {
    let (p, _) = shop()?;
    let mut e = editor_with(p);
    e.path = Some("/home/you/web-shop/architecture.yaml".into());
    let mut h = harness(e);
    h.run();
    let editor = &h.state().editor;
    let mut dialog = whiteboxed::ui::export::ExportDialog::default();
    dialog.start(editor);
    dialog.set_folder("/home/you/web-shop/docs/arc42".into());
    h.state_mut().export = dialog;
    h.run();
    save(&mut h, "export")
}
