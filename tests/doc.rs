use whiteboxed::doc::{self, DocFormat};
use whiteboxed::model::{BlockId, BlockKind, BlockSpec, Direction, End, Project, Side};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn shop() -> Result<(Project, BlockId), Box<dyn std::error::Error>> {
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
    p.connect_new(
        shop,
        Side::Right,
        &spec("Payment", BlockKind::ExternalSystem),
        Direction::Bi,
        "REST | JSON",
    )?;
    p.connect_new(
        shop,
        Side::Bottom,
        &spec("Warehouse", BlockKind::ExternalSystem),
        Direction::Out,
        "shipping orders",
    )?;
    p.set_motivation(None, "Who talks to the shop.")?;
    p.set_responsibility(user, "Buys things.")?;
    let warehouse = p
        .blocks_in(None)
        .find(|(_, b)| b.name == "Warehouse")
        .map(|(id, _)| id)
        .ok_or("warehouse")?;
    p.set_responsibility(warehouse, "Ships what was bought.")?;
    p.set_responsibility(shop, "Sells things.\nShips them.")?;
    let ui = p.add_block(Some(shop), &spec("Storefront", BlockKind::Ui))?;
    p.set_responsibility(ui, "Shows the catalog.")?;
    p.connect_new(
        ui,
        Side::Right,
        &spec("Orders", BlockKind::Component),
        Direction::Out,
        "place order",
    )?;
    p.attach(buy, End::B, shop, ui, Side::Left)?;
    p.set_motivation(Some(shop), "Split by capability.")?;
    Ok((p, shop))
}

#[test]
fn the_context_lists_partners_with_input_and_output() -> TestResult {
    let (p, _) = shop()?;
    let md = doc::diagram_doc(&p, None, "context.svg", DocFormat::Markdown);
    assert!(
        md.starts_with("### Context\n\n![Context view](<context.svg>)\n\nWho talks to the shop.\n")
    );
    assert!(md.contains(
        "| Partner   | Description            | Input        | Output          |\n\
         |-----------|------------------------|--------------|-----------------|\n\
         | Customer  | Buys things.           | orders       | \u{2013}               |\n\
         | Payment   | \u{2013}                      | REST \\| JSON | REST \\| JSON    |\n\
         | Warehouse | Ships what was bought. | \u{2013}            | shipping orders |\n"
    ));
    assert!(md.contains("| Web Shop | Sells things.<br>Ships them. |"));
    Ok(())
}

#[test]
fn a_whitebox_has_blocks_external_interfaces_and_internal_relations() -> TestResult {
    let (p, shop) = shop()?;
    let adoc = doc::diagram_doc(
        &p,
        Some(shop),
        "context - Web Shop.svg",
        DocFormat::AsciiDoc,
    );
    assert!(adoc.starts_with(
        "=== Whitebox Web Shop\n\nimage::context - Web Shop.svg[Whitebox Web Shop]\n\n"
    ));
    assert!(adoc.contains("==== Motivation\n\nSplit by capability.\n"));
    assert!(adoc.contains("|Storefront\n|Shows the catalog.\n"));
    assert!(adoc.contains("|Orders\n|\u{2013}\n"));
    assert!(adoc.contains("==== External interfaces"));
    assert!(adoc.contains("|Customer\n|Storefront\n|in\n|orders\n"));
    assert!(adoc.contains("|Payment\n|not assigned\n|bi\n|REST \\| JSON\n"));
    assert!(adoc.contains("|Warehouse\n|not assigned\n|out\n|shipping orders\n"));
    assert!(adoc.contains("==== Internal relations"));
    assert!(adoc.contains("|Storefront\n|Orders\n|out\n|place order\n"));
    Ok(())
}

#[test]
fn the_index_includes_or_links_every_diagram_in_level_order() -> TestResult {
    let (p, shop) = shop()?;
    let order: Vec<_> = doc::diagrams(&p);
    assert_eq!(order, vec![None, Some(shop)]);
    let entries = vec![
        (None, "context".to_owned()),
        (Some(shop), "context - Web Shop".to_owned()),
    ];
    let adoc = doc::index_doc(&p, "Shop", &entries, DocFormat::AsciiDoc);
    assert_eq!(
        adoc,
        "= Shop\n\n== Context and scope\n\ninclude::context.adoc[]\n\n\
         == Building block view\n\ninclude::context - Web Shop.adoc[]\n\n"
    );
    let md = doc::index_doc(&p, "Shop", &entries, DocFormat::Markdown);
    assert!(md.contains("- [Context](<context.md>)"));
    assert!(md.contains("- [Whitebox Web Shop](<context - Web Shop.md>)"));
    Ok(())
}

#[test]
#[ignore = "prints the sample documents"]
fn print_samples() -> TestResult {
    let (p, shop) = shop()?;
    for f in [DocFormat::AsciiDoc, DocFormat::Markdown] {
        println!("{}", doc::diagram_doc(&p, None, "context.svg", f));
        println!(
            "{}",
            doc::diagram_doc(&p, Some(shop), "context - Web Shop.svg", f)
        );
    }
    Ok(())
}

#[test]
fn typed_text_cannot_inject_markup_into_the_export() -> TestResult {
    let mut p = Project::new();
    let shop = p.add_block(None, &BlockSpec::new("Shop", BlockKind::Component))?;
    p.set_motivation(
        None,
        "include::/etc/passwd[]\n+++<script>alert(1)</script>+++\npass:[<b>x</b>] {user-home} C:\\dir",
    )?;
    p.set_responsibility(
        shop,
        "[click](javascript:alert(1)) <img src=x onerror=alert(1)>",
    )?;
    let adoc = doc::diagram_doc(&p, None, "context.svg", DocFormat::AsciiDoc);
    assert!(adoc.contains("\\include::/etc/passwd[]"));
    assert!(!adoc.contains("+++"));
    assert!(adoc.contains("{plus}{plus}{plus}<script>"));
    assert!(adoc.contains("\\pass:[<b>x</b>]"));
    assert!(adoc.contains("\\{user-home}"));
    assert!(adoc.contains("C:{backslash}dir"));
    let md = doc::diagram_doc(&p, None, "context.svg", DocFormat::Markdown);
    assert!(!md.contains("<script>"));
    assert!(!md.contains("<img"));
    assert!(md.contains("&lt;script&gt;"));
    assert!(md.contains("\\[click\\](javascript:alert(1))"));
    Ok(())
}

#[test]
fn a_line_without_arrows_counts_as_input_and_output() -> TestResult {
    let mut p = Project::new();
    let spec = BlockSpec::new;
    let shop = p.add_block(None, &spec("Shop", BlockKind::Component))?;
    let (_, rel) = p.connect_new(
        shop,
        Side::Right,
        &spec("Ops Team", BlockKind::Person),
        Direction::Undirected,
        "runbooks",
    )?;
    let md = doc::diagram_doc(&p, None, "context.svg", DocFormat::Markdown);
    let row = md
        .lines()
        .find(|l| l.starts_with("| Ops Team"))
        .ok_or("partner row")?;
    let cells: Vec<&str> = row.split('|').map(str::trim).collect();
    assert_eq!(cells[3], "runbooks", "input: {row}");
    assert_eq!(cells[4], "runbooks", "output: {row}");
    // Inside the whitebox the interface reads "none".
    let inner = p.add_block(Some(shop), &spec("Docs", BlockKind::Component))?;
    p.attach(rel, End::A, shop, inner, Side::Right)?;
    let md = doc::diagram_doc(&p, Some(shop), "shop.svg", DocFormat::Markdown);
    assert!(
        md.lines()
            .any(|l| l.contains("runbooks") && l.contains("| none")),
        "{md}"
    );
    Ok(())
}
