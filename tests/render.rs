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
fn the_canvas_paints_a_look() -> TestResult {
    use whiteboxed::look::{Look, LookId, LookMode};
    let (mut p, shop) = shop()?;
    let svg = r##"<svg viewBox="0 0 40 20"><rect width="40" height="20" fill="#ff0000"/></svg>"##;
    let id = LookId("red".into());
    p.looks
        .insert(id.clone(), Look::new("Red", LookMode::Inside, svg, 0)?);
    p.blocks.get_mut(&shop).ok_or("shop")?.look = Some(id);
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
        close(got, Rgb(0xff, 0, 0)),
        "expected red inside the box, got {got}"
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
            listen: whiteboxed::mcp::listen::Listen::Local,
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

#[test]
#[ignore = "writes target/sample.html"]
fn sample_html() -> TestResult {
    let (p, _) = shop()?;
    std::fs::write(
        "target/sample.html",
        whiteboxed::html::html_doc(&p, "Web Shop"),
    )?;
    Ok(())
}

#[test]
#[ignore = "writes docs/screenshots (needs wgpu)"]
fn doc_screenshot_split() -> TestResult {
    let (p, shop) = shop()?;
    let mut e = editor_with(p);
    let rel = *e
        .project
        .relations
        .iter()
        .find(|(_, r)| r.text == "orders via browser")
        .ok_or("relation")?
        .0;
    let orders = e
        .project
        .blocks
        .iter()
        .find(|(_, b)| b.name == "Orders")
        .map(|(id, _)| *id)
        .ok_or("orders")?;
    e.project.attach(rel, End::B, shop, orders, Side::Top)?;
    e.start_split(rel);
    if let Some(whiteboxed::editor::Popup::Split(form)) = &mut e.popup {
        form.pairs[0].on = true;
        form.pairs[0].text = "browse".into();
        form.pairs[1].on = true;
        form.pairs[1].text = "order".into();
    }
    let mut h = harness(e);
    h.run();
    save(&mut h, "split")
}

fn right_click(h: &mut Harness<'_, App>, pos: Pos2) {
    h.event(Event::PointerMoved(pos));
    h.step();
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.step();
    }
    h.run();
}

#[test]
#[ignore = "writes docs/screenshots (needs wgpu)"]
fn doc_screenshot_readability() -> TestResult {
    let (mut p, shop) = shop()?;
    for name in ["Search", "Reviews", "Recommendations", "Wishlist"] {
        p.add_block(Some(shop), &BlockSpec::new(name, BlockKind::Component))?;
    }
    let mut e = editor_with(p);
    e.open_diagram(Some(shop));
    let mut h = harness(e);
    h.run();
    save(&mut h, "readability")
}

#[test]
#[ignore = "writes docs/screenshots (needs wgpu)"]
fn doc_screenshot_restructure_menu() -> TestResult {
    let (p, shop) = shop()?;
    let id = |name: &str| {
        p.blocks
            .iter()
            .find(|(_, b)| b.name == name)
            .map(|(id, _)| *id)
            .ok_or(format!("no {name}"))
    };
    let (orders, db) = (id("Orders")?, id("Order DB")?);
    let mut e = editor_with(p.clone());
    e.open_diagram(Some(shop));
    e.selected = Some(orders);
    e.also_selected.insert(db);
    let mut h = harness(e);
    h.run();
    let r = h.state_mut().editor.layout().block(db).ok_or("db")?.rect;
    let at = screen(&h, r.center())?;
    right_click(&mut h, at);
    save(&mut h, "restructure")
}

/// A login flow on one page and a network sketch on another.
fn doodle() -> Result<Editor, Box<dyn std::error::Error>> {
    use whiteboxed::model::{Cell, Placement, Rgb};
    let mut p = Project::new_doodle();
    let page = p.pages()[0];
    p.edit_block(page, &BlockSpec::new("Login flow", BlockKind::Page))?;
    let shape = |text: &str, kind| BlockSpec::new(text, kind);
    let user = p.add_block(Some(page), &shape("User", BlockKind::Person))?;
    let (start, _) = p.connect_new(
        user,
        Side::Right,
        &shape("Start", BlockKind::Circle),
        Direction::Out,
        "opens",
    )?;
    let (form, _) = p.connect_new(
        start,
        Side::Right,
        &shape("Enter name\nand password", BlockKind::Rounded),
        Direction::Out,
        "",
    )?;
    let check = BlockSpec {
        color: Some(Rgb(0xff, 0xe6, 0x99)),
        ..shape("Password\nvalid?", BlockKind::Diamond)
    };
    let (valid, _) = p.connect_new(form, Side::Right, &check, Direction::Out, "submit")?;
    let (session, _) = p.connect_new(
        valid,
        Side::Right,
        &shape("Create session", BlockKind::Rounded),
        Direction::Out,
        "yes",
    )?;
    let (_, _) = p.connect_new(
        valid,
        Side::Bottom,
        &shape("Show error", BlockKind::Rounded),
        Direction::Out,
        "no",
    )?;
    p.connect_new(
        session,
        Side::Bottom,
        &shape("Sessions", BlockKind::Database),
        Direction::Out,
        "store",
    )?;
    p.connect_new(
        session,
        Side::Right,
        &shape("Welcome\npage", BlockKind::Document),
        Direction::Out,
        "",
    )?;
    let note = p.add_block(
        Some(page),
        &BlockSpec {
            color: Some(Rgb(0xff, 0xf4, 0xb8)),
            ..shape("After 5 failures:\nlock for 10 minutes", BlockKind::Note)
        },
    )?;
    p.move_block(note, Cell::new(1, 2))?;
    let net = p.add_page("Network")?;
    let internet = p.add_block(Some(net), &shape("Internet", BlockKind::Cloud))?;
    let (lb, _) = p.connect_new(
        internet,
        Side::Right,
        &shape("Load\nbalancer", BlockKind::Component),
        Direction::Out,
        "HTTPS",
    )?;
    let (app, _) = p.connect_new(
        lb,
        Side::Right,
        &shape("App", BlockKind::Component),
        Direction::Out,
        "",
    )?;
    let health = p.connect_existing(
        app,
        Side::Left,
        lb,
        Direction::Out,
        "health",
        Placement::Keep,
    )?;
    p.set_relation_pattern(health, whiteboxed::model::LinePattern::Dotted)?;
    let (_, logs) = p.connect_new(
        app,
        Side::Bottom,
        &shape("Log store", BlockKind::Database),
        Direction::Out,
        "logs",
    )?;
    p.set_relation_pattern(logs, whiteboxed::model::LinePattern::Dashed)?;
    let store = *p
        .blocks
        .iter()
        .find(|(_, b)| b.name == "Log store")
        .ok_or("log store")?
        .0;
    p.add_group(&[lb], "DMZ", None)?;
    p.add_group(
        &[lb, app, store],
        "Cloud account",
        Some(Rgb(0xe8, 0xf1, 0xfb)),
    )?;
    let mut e = editor_with(p);
    e.open_diagram(Some(page));
    Ok(e)
}

#[test]
#[ignore = "writes docs/screenshots (needs wgpu)"]
fn doc_screenshot_doodle() -> TestResult {
    let mut h = harness(doodle()?);
    h.run();
    save(&mut h, "doodle")
}

#[test]
#[ignore = "writes docs/screenshots (needs wgpu)"]
fn doc_screenshot_doodle_network() -> TestResult {
    let mut e = doodle()?;
    let net = e.project.pages()[1];
    e.open_diagram(Some(net));
    let mut h = harness(e);
    h.run();
    save(&mut h, "doodle-network")
}

#[test]
#[ignore = "writes target/doodle.html"]
fn sample_doodle_html() -> TestResult {
    let e = doodle()?;
    std::fs::write(
        "target/doodle.html",
        whiteboxed::html::html_doc(&e.project, "Login and network"),
    )?;
    Ok(())
}

const STREAM_LOOK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 60">
<defs><linearGradient id="bg" x1="0" y1="0" x2="0" y2="1">
<stop offset="0" stop-color="#eef6ff"/><stop offset="1" stop-color="#cfe3fb"/></linearGradient></defs>
<rect x="1" y="1" width="118" height="58" rx="10" fill="url(#bg)" stroke="#2c4f9e" stroke-width="2"/>
<g stroke="#2c4f9e" stroke-width="1.5"><line x1="14" y1="14" x2="14" y2="46"/>
<line x1="14" y1="30" x2="22" y2="30"/></g>
<g fill="#ffffff" stroke="#2c4f9e" stroke-width="1.5">
<circle cx="14" cy="11" r="3.5"/><circle cx="14" cy="49" r="3.5"/><circle cx="25" cy="30" r="3.5"/></g>
</svg>"##;

const SERVER_LOOK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 48">
<g fill="#5b8def" stroke="#2c4f9e" stroke-width="1.5">
<rect x="4" y="2" width="32" height="12" rx="2"/><rect x="4" y="18" width="32" height="12" rx="2"/>
<rect x="4" y="34" width="32" height="12" rx="2"/></g>
<g fill="#b9f6ca"><circle cx="10" cy="8" r="2"/><circle cx="10" cy="24" r="2"/><circle cx="10" cy="40" r="2"/></g>
</svg>"##;

const VAULT_LOOK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">
<rect x="3" y="3" width="26" height="26" rx="4" fill="#f4b400" stroke="#8a6500" stroke-width="2"/>
<circle cx="16" cy="16" r="7" fill="none" stroke="#8a6500" stroke-width="2.5"/>
<line x1="16" y1="9" x2="16" y2="13" stroke="#8a6500" stroke-width="2.5"/>
</svg>"##;

/// A doodle of an event flow whose special nodes wear looks from the library.
fn looks_doodle() -> Result<Editor, Box<dyn std::error::Error>> {
    use whiteboxed::look::{Library, LookMode};
    let mut e = Editor::new_doodle(None);
    e.set_library(Library::default(), None);
    let stream = e.add_look("Event stream", LookMode::Inside, STREAM_LOOK)?;
    let server = e.add_look("Server", LookMode::Below, SERVER_LOOK)?;
    let vault = e.add_look("Vault", LookMode::Icon, VAULT_LOOK)?;
    let page = e.project.pages()[0];
    e.open_diagram(Some(page));
    let p = &mut e.project;
    let shape = |text: &str| BlockSpec::new(text, BlockKind::Component);
    let api = p.add_block(Some(page), &shape("Order API"))?;
    let (bus, _) = p.connect_new(
        api,
        Side::Right,
        &shape("orders"),
        Direction::Out,
        "publish",
    )?;
    let (worker, _) = p.connect_new(
        bus,
        Side::Right,
        &shape("Fulfilment"),
        Direction::Out,
        "consume",
    )?;
    let (secrets, _) = p.connect_new(
        worker,
        Side::Bottom,
        &shape("Secrets"),
        Direction::Out,
        "reads keys",
    )?;
    for (block, look) in [
        (api, &server),
        (bus, &stream),
        (worker, &server),
        (secrets, &vault),
    ] {
        let copy = e.look_copy(look).ok_or("look")?;
        e.project.wear_look(block, Some(copy))?;
    }
    Ok(e)
}

#[test]
#[ignore = "writes docs/screenshots (needs wgpu)"]
fn doc_screenshot_looks() -> TestResult {
    let mut h = harness(looks_doodle()?);
    h.run();
    save(&mut h, "looks")
}

#[test]
#[ignore = "writes docs/screenshots (needs wgpu)"]
fn doc_screenshot_looks_dialog() -> TestResult {
    let mut h = harness(looks_doodle()?);
    h.run();
    h.state_mut().looks_dialog.start();
    h.run();
    save(&mut h, "looks-dialog")
}

const CAT_LOOK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 92">
<g stroke="#6b3a10" stroke-width="3" stroke-linejoin="round">
<polygon points="16,44 24,4 48,28" fill="#f4a340"/><polygon points="84,44 76,4 52,28" fill="#f4a340"/>
<ellipse cx="50" cy="56" rx="38" ry="33" fill="#f4a340"/></g>
<g fill="#f7b6c2"><polygon points="24,32 27,14 40,27"/><polygon points="76,32 73,14 60,27"/></g>
<g stroke="#c9761f" stroke-width="3" stroke-linecap="round">
<line x1="44" y1="26" x2="46" y2="35"/><line x1="50" y1="25" x2="50" y2="35"/><line x1="56" y1="26" x2="54" y2="35"/></g>
<g fill="#9ccc65" stroke="#33691e" stroke-width="1.5"><ellipse cx="35" cy="52" rx="7" ry="8.5"/><ellipse cx="65" cy="52" rx="7" ry="8.5"/></g>
<g fill="#1b1b1b"><ellipse cx="35" cy="52" rx="1.8" ry="6.5"/><ellipse cx="65" cy="52" rx="1.8" ry="6.5"/></g>
<g fill="#ffffff"><circle cx="37.5" cy="48" r="1.6"/><circle cx="67.5" cy="48" r="1.6"/></g>
<polygon points="45,64 55,64 50,70" fill="#e57373" stroke="#6b3a10" stroke-width="1.5" stroke-linejoin="round"/>
<path d="M50 70 Q46 77 40 73 M50 70 Q54 77 60 73" fill="none" stroke="#6b3a10" stroke-width="2" stroke-linecap="round"/>
<g stroke="#6b3a10" stroke-width="1.5" stroke-linecap="round">
<line x1="30" y1="66" x2="4" y2="62"/><line x1="30" y1="70" x2="5" y2="72"/>
<line x1="70" y1="66" x2="96" y2="62"/><line x1="70" y1="70" x2="95" y2="72"/></g>
</svg>"##;

const MOUSE_LOOK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 112 72">
<path d="M88 56 C104 60 110 44 100 34 C94 28 98 20 106 18" fill="none" stroke="#d98c9a" stroke-width="3" stroke-linecap="round"/>
<ellipse cx="64" cy="50" rx="30" ry="19" fill="#b8b8b8" stroke="#555555" stroke-width="2.5"/>
<g stroke="#555555" stroke-width="2.5"><circle cx="34" cy="20" r="12" fill="#a8a8a8"/><circle cx="54" cy="22" r="11" fill="#a8a8a8"/></g>
<g fill="#f7b6c2"><circle cx="34" cy="20" r="7"/><circle cx="54" cy="22" r="6.5"/></g>
<ellipse cx="36" cy="44" rx="20" ry="15" fill="#c4c4c4" stroke="#555555" stroke-width="2.5"/>
<circle cx="31" cy="40" r="3.5" fill="#222222"/><circle cx="32.2" cy="38.8" r="1.1" fill="#ffffff"/>
<circle cx="16.5" cy="48" r="3.5" fill="#e57373" stroke="#555555" stroke-width="1"/>
<g stroke="#555555" stroke-width="1.2" stroke-linecap="round">
<line x1="20" y1="50" x2="4" y2="46"/><line x1="20" y1="52" x2="5" y2="56"/><line x1="22" y1="53" x2="10" y2="62"/></g>
<g fill="#b8b8b8" stroke="#555555" stroke-width="2"><ellipse cx="52" cy="68" rx="6" ry="3"/><ellipse cx="78" cy="67" rx="6" ry="3"/></g>
</svg>"##;

/// The README example: a cat and a mouse as custom looks in a doodle.
fn cat_and_mouse() -> Result<Editor, Box<dyn std::error::Error>> {
    use whiteboxed::look::{Library, LookMode};
    use whiteboxed::model::{Cell, LinePattern, Placement, Rgb};
    let mut e = Editor::new_doodle(None);
    e.set_library(Library::default(), None);
    let cat_look = e.add_look("Cat", LookMode::Below, CAT_LOOK)?;
    let mouse_look = e.add_look("Mouse", LookMode::Below, MOUSE_LOOK)?;
    let page = e.project.pages()[0];
    e.open_diagram(Some(page));
    let p = &mut e.project;
    p.edit_block(page, &BlockSpec::new("Kitchen", BlockKind::Page))?;
    let shape = |text: &str, kind| BlockSpec::new(text, kind);
    let mouse = p.add_block(Some(page), &shape("Mouse", BlockKind::Component))?;
    let asleep = BlockSpec {
        color: Some(Rgb(0xff, 0xe6, 0x99)),
        ..shape("Cat\nasleep?", BlockKind::Diamond)
    };
    let (check, _) = p.connect_new(mouse, Side::Right, &asleep, Direction::Out, "sniffs")?;
    let cheese = BlockSpec {
        color: Some(Rgb(0xff, 0xd8, 0x4d)),
        ..shape("Grab\nthe cheese", BlockKind::Rounded)
    };
    p.connect_new(check, Side::Right, &cheese, Direction::Out, "yes")?;
    p.connect_new(
        check,
        Side::Bottom,
        &shape("Back into\nthe hole", BlockKind::Rounded),
        Direction::Out,
        "no",
    )?;
    let cat = p.add_block_at(
        Some(page),
        &shape("Tom", BlockKind::Component),
        Cell::new(0, 1),
    )?;
    let chase = p.connect_existing(
        cat,
        Side::Top,
        mouse,
        Direction::Out,
        "chases",
        Placement::Keep,
    )?;
    p.set_relation_pattern(chase, LinePattern::Dashed)?;
    p.add_block_at(
        Some(page),
        &BlockSpec {
            color: Some(Rgb(0xff, 0xf4, 0xb8)),
            ..shape("Tom only\npretends to sleep", BlockKind::Note)
        },
        Cell::new(2, 1),
    )?;
    let mouse_copy = e.look_copy(&mouse_look).ok_or("mouse")?;
    let cat_copy = e.look_copy(&cat_look).ok_or("cat")?;
    e.project.wear_look(mouse, Some(mouse_copy))?;
    e.project.wear_look(cat, Some(cat_copy))?;
    Ok(e)
}

#[test]
#[ignore = "writes docs/screenshots (needs wgpu)"]
fn doc_screenshot_cat_and_mouse() -> TestResult {
    let mut h = harness(cat_and_mouse()?);
    h.run();
    save(&mut h, "cat-and-mouse")
}
