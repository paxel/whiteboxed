use whiteboxed::html;
use whiteboxed::model::{BlockKind, BlockSpec, Direction, End, Project, Side};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A shop with partners, a whitebox and an inner whitebox.
fn shop() -> Result<Project, Box<dyn std::error::Error>> {
    let mut p = Project::new();
    let spec = BlockSpec::new;
    let user = p.add_block(None, &spec("Customer", BlockKind::Person))?;
    let (shop, buy) = p.connect_new(
        user,
        Side::Right,
        &spec("Web Shop", BlockKind::Component),
        Direction::Out,
        "orders",
    )?;
    p.set_responsibility(shop, "Sells things.\nShips them.")?;
    let ui = p.add_block(Some(shop), &spec("Storefront", BlockKind::Ui))?;
    let (orders, _) = p.connect_new(
        ui,
        Side::Right,
        &spec("Orders", BlockKind::Component),
        Direction::Out,
        "place order",
    )?;
    p.attach(buy, End::B, shop, ui, Side::Left)?;
    p.add_block(Some(orders), &spec("Validator", BlockKind::Component))?;
    Ok(p)
}

#[test]
fn every_diagram_is_a_linked_section() -> TestResult {
    let p = shop()?;
    let page = html::html_doc(&p, "Web Shop");
    let id = |name: &str| -> Result<u64, Box<dyn std::error::Error>> {
        Ok(p.blocks
            .iter()
            .find(|(_, b)| b.name == name)
            .ok_or(name.to_owned())?
            .0
            .0)
    };
    let (shop, orders, customer) = (id("Web Shop")?, id("Orders")?, id("Customer")?);
    for section in [
        "id=\"context\"",
        &format!("id=\"wb-{shop}\""),
        &format!("id=\"wb-{orders}\""),
    ] {
        assert!(page.contains(section), "{section}");
    }
    // The tree nests Orders under Web Shop under Context.
    assert!(page.contains(&format!(
        "<li><a href=\"#wb-{shop}\">Web Shop</a><ul><li><a href=\"#wb-{orders}\">Orders</a></li></ul>"
    )));
    // Breadcrumb of the inner whitebox.
    assert!(page.contains(&format!(
        "<a href=\"#context\">Context</a> <span>›</span> <a href=\"#wb-{shop}\">Web Shop</a> <span>›</span> <a href=\"#wb-{orders}\">Orders</a>"
    )));
    // A box with a whitebox opens it; table rows carry their box or relation.
    assert!(page.contains(&format!("data-box=\"{shop}\" data-open=\"wb-{shop}\"")));
    assert!(page.contains(&format!("<tr data-box=\"{customer}\">")));
    assert!(page.contains("<tr data-rel="));
    assert!(page.contains("Sells things.<br>Ships them."));
    // The frame end in the shop's whitebox leads to the customer in the context view.
    assert!(page.contains(&format!(
        "data-partner=\"{customer}\" data-goto=\"context\""
    )));
    Ok(())
}

#[test]
fn the_page_needs_nothing_from_outside_and_escapes_names() -> TestResult {
    let mut p = shop()?;
    let evil = p.add_block(
        None,
        &BlockSpec::new("<script>alert(1)</script>", BlockKind::Component),
    )?;
    p.set_responsibility(evil, "\"quoted\" & <b>bold</b>")?;
    let page = html::html_doc(&p, "A <b> & \"title\"");
    assert!(!page.contains("<script>alert"));
    assert!(!page.contains("<b>bold"));
    assert!(page.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(page.contains("<title>A &lt;b&gt; &amp; &quot;title&quot;</title>"));
    // Only the SVG namespace names a URL; no script, style, font or image is fetched.
    let without_ns = page.replace("http://www.w3.org/2000/svg", "");
    assert!(!without_ns.contains("http://") && !without_ns.contains("https://"));
    assert!(page.contains("@font-face"));
    assert_eq!(page.matches("<script>").count(), 1);
    Ok(())
}

#[test]
fn a_bundled_line_points_at_every_relation_it_stands_for() -> TestResult {
    use whiteboxed::model::Placement;
    let mut p = Project::new();
    let a = p.add_block(None, &BlockSpec::new("A", BlockKind::Component))?;
    let (b, first) = p.connect_new(
        a,
        Side::Right,
        &BlockSpec::new("B", BlockKind::Component),
        Direction::Out,
        "provide",
    )?;
    let second =
        p.connect_existing(b, Side::Left, a, Direction::Out, "require", Placement::Keep)?;
    let page = html::html_doc(&p, "AB");
    assert!(page.contains(&format!(
        "data-rel=\"{}\" data-also=\"{}\"",
        first.0, second.0
    )));
    Ok(())
}
