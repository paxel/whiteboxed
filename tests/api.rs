use serde_json::{Value, json};
use whiteboxed::api::{self, ApiError, Output};
use whiteboxed::editor::Editor;
use whiteboxed::model::{BlockId, Cell, Direction};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn call(e: &mut Editor, tool: &str, args: Value) -> Result<Value, ApiError> {
    match api::call(e, tool, args)? {
        Output::Json(v) => Ok(v),
        Output::Png(_) => Ok(json!("png")),
    }
}

fn id(v: &Value) -> Result<u64, Box<dyn std::error::Error>> {
    v["id"]
        .as_u64()
        .ok_or_else(|| format!("no id in {v}").into())
}

/// Context view with a shop, a customer and a payment provider.
fn shop(e: &mut Editor) -> Result<u64, Box<dyn std::error::Error>> {
    let shop = call(
        e,
        "add_box",
        json!({"name": "Web Shop", "kind": "component", "responsibility": "Sells."}),
    )?;
    call(
        e,
        "connect_new",
        json!({"from": "Web Shop", "side": "left", "name": "Customer", "kind": "person", "direction": "in", "text": "orders"}),
    )?;
    call(
        e,
        "connect_new",
        json!({"from": ["Web Shop"], "side": "right", "name": "Payment", "kind": "external_system", "direction": "bi", "text": "REST"}),
    )?;
    id(&shop)
}

#[test]
fn every_tool_has_an_object_schema_and_a_description() {
    let tools = api::tools();
    assert_eq!(tools.len(), 17);
    for t in tools {
        assert_eq!(t.schema.get("type"), Some(&json!("object")), "{}", t.name);
        assert!(!t.description.is_empty());
    }
    assert!(api::INSTRUCTIONS.contains("render_diagram"));
}

#[test]
fn building_the_context_view_with_paths_and_ids() -> TestResult {
    let mut e = Editor::new(None);
    let shop = shop(&mut e)?;
    assert_eq!(e.project.blocks.len(), 3);
    assert_eq!(e.project.relations.len(), 2);
    let model = call(&mut e, "get_model", json!({}))?;
    let names: Vec<_> = model["boxes"]
        .as_array()
        .ok_or("boxes")?
        .iter()
        .map(|b| b["name"].clone())
        .collect();
    assert_eq!(
        names,
        vec![json!("Web Shop"), json!("Customer"), json!("Payment")]
    );
    assert_eq!(model["boxes"][0]["responsibility"], "Sells.");
    // Addressing by id still works after a rename.
    call(&mut e, "edit_box", json!({"box": shop, "name": "Shop"}))?;
    assert_eq!(e.project.block(BlockId(shop))?.name, "Shop");
    let d = call(&mut e, "get_diagram", json!({}))?;
    assert_eq!(d["level"], 0);
    assert_eq!(d["lines"].as_array().ok_or("lines")?.len(), 2);
    Ok(())
}

#[test]
fn every_call_is_one_undo_step() -> TestResult {
    let mut e = Editor::new(None);
    call(
        &mut e,
        "add_box",
        json!({"name": "A", "kind": "component", "responsibility": "Does A."}),
    )?;
    call(
        &mut e,
        "connect_new",
        json!({"from": "A", "side": "right", "name": "B", "kind": "database", "responsibility": "Stores."}),
    )?;
    e.undo();
    assert_eq!(e.project.blocks.len(), 1);
    e.undo();
    assert!(e.project.blocks.is_empty());
    assert!(!e.can_undo());
    Ok(())
}

#[test]
fn errors_come_back_as_readable_text_and_change_nothing() -> TestResult {
    let mut e = Editor::new(None);
    shop(&mut e)?;
    let before = e.project.clone();
    let dup = call(
        &mut e,
        "add_box",
        json!({"name": "web shop", "kind": "component"}),
    );
    assert_eq!(
        dup,
        Err(ApiError::Rejected(
            "\"web shop\" already exists in this diagram".into()
        ))
    );
    let neighbour = call(
        &mut e,
        "add_box",
        json!({"diagram": "Web Shop", "name": "Bank", "kind": "external_system"}),
    );
    assert_eq!(
        neighbour,
        Err(ApiError::Rejected(
            "external system boxes only exist in the context view".into()
        ))
    );
    assert!(matches!(
        call(&mut e, "add_box", json!({"name": 3})),
        Err(ApiError::BadArguments(_))
    ));
    assert_eq!(
        call(&mut e, "frobnicate", json!({})),
        Err(ApiError::UnknownTool("frobnicate".into()))
    );
    assert_eq!(
        call(&mut e, "edit_box", json!({"box": "Nope"})),
        Err(ApiError::Rejected("no box \"Nope\"".into()))
    );
    assert_eq!(e.project, before);
    Ok(())
}

#[test]
fn names_with_slashes_resolve_and_ambiguity_is_reported() -> TestResult {
    let mut e = Editor::new(None);
    call(
        &mut e,
        "add_box",
        json!({"name": "Shop", "kind": "component"}),
    )?;
    let ob = call(
        &mut e,
        "add_box",
        json!({"diagram": "Shop", "name": "Orders/Billing", "kind": "component"}),
    )?;
    let found = call(
        &mut e,
        "set_responsibility",
        json!({"box": "Shop/Orders/Billing", "text": "Bills."}),
    )?;
    assert_eq!(found["id"], ob["id"]);
    assert_eq!(found["path"], json!(["Shop", "Orders/Billing"]));
    // "Shop/Orders" as a name next to "Shop" with an inner "Orders" makes the text ambiguous.
    call(
        &mut e,
        "add_box",
        json!({"name": "Shop/Orders", "kind": "component"}),
    )?;
    call(
        &mut e,
        "add_box",
        json!({"diagram": ["Shop"], "name": "Orders", "kind": "component"}),
    )?;
    let amb = call(
        &mut e,
        "set_responsibility",
        json!({"box": "Shop/Orders", "text": "x"}),
    );
    assert!(matches!(amb, Err(ApiError::Rejected(m)) if m.contains("ambiguous")));
    // A list of names is never ambiguous.
    call(
        &mut e,
        "set_responsibility",
        json!({"box": ["Shop/Orders"], "text": "outer"}),
    )?;
    Ok(())
}

#[test]
fn connect_existing_moves_by_default_and_keeps_on_request() -> TestResult {
    let mut e = Editor::new(None);
    call(&mut e, "add_box", json!({"name": "A", "kind": "component"}))?;
    call(
        &mut e,
        "connect_new",
        json!({"from": "A", "side": "right", "name": "B", "kind": "component"}),
    )?;
    call(
        &mut e,
        "connect_new",
        json!({"from": "B", "side": "right", "name": "C", "kind": "component"}),
    )?;
    let b = |e: &Editor| {
        e.project
            .blocks
            .values()
            .find(|x| x.name == "B")
            .map(|x| x.cell)
    };
    call(
        &mut e,
        "connect_existing",
        json!({"from": "C", "side": "top", "to": "B", "placement": "keep"}),
    )?;
    assert_eq!(b(&e), Some(Cell::new(1, 0)));
    e.undo();
    call(
        &mut e,
        "connect_existing",
        json!({"from": "C", "side": "top", "to": "B"}),
    )?;
    assert_eq!(b(&e), Some(Cell::new(2, -1)));
    Ok(())
}

#[test]
fn whiteboxes_attach_inherited_relations_and_stubs_connect_later() -> TestResult {
    let mut e = Editor::new(None);
    shop(&mut e)?;
    call(
        &mut e,
        "add_box",
        json!({"diagram": "Web Shop", "name": "Storefront", "kind": "ui"}),
    )?;
    let d = call(&mut e, "get_diagram", json!({"diagram": "Web Shop"}))?;
    assert_eq!(d["dangling"], 2);
    let customer_rel = d["lines"]
        .as_array()
        .ok_or("lines")?
        .iter()
        .find(|l| {
            l["a"]["partner_outside"] == "Customer" || l["b"]["partner_outside"] == "Customer"
        })
        .and_then(|l| l["relation"].as_u64())
        .ok_or("customer line")?;
    call(
        &mut e,
        "attach",
        json!({"relation": customer_rel, "whitebox": "Web Shop", "to": "Web Shop/Storefront"}),
    )?;
    let d = call(&mut e, "get_diagram", json!({"diagram": "Web Shop"}))?;
    assert_eq!(d["dangling"], 1);

    let stub = call(
        &mut e,
        "add_stub",
        json!({"from": "Web Shop/Storefront", "side": "bottom", "text": "metrics"}),
    )?;
    let stub_id = id(&stub)?;
    let mon = call(
        &mut e,
        "add_box",
        json!({"name": "Monitoring", "kind": "external_system"}),
    )?;
    let r = call(
        &mut e,
        "connect_open_end",
        json!({"relation": stub_id, "to": id(&mon)?}),
    )?;
    assert_eq!(r["diagram"], Value::Null);
    assert_eq!(r["b"][0]["name"], "Monitoring");
    Ok(())
}

#[test]
fn deleting_needs_recursive_for_content_and_relations_can_be_detached() -> TestResult {
    let mut e = Editor::new(None);
    shop(&mut e)?;
    call(
        &mut e,
        "add_box",
        json!({"diagram": "Web Shop", "name": "Storefront", "kind": "ui"}),
    )?;
    let rel = *e.project.relations.keys().next().ok_or("relation")?;
    call(
        &mut e,
        "attach",
        json!({"relation": rel.0, "whitebox": "Web Shop", "to": "Web Shop/Storefront"}),
    )?;
    call(
        &mut e,
        "delete_relation",
        json!({"relation": rel.0, "detach_in": "Web Shop"}),
    )?;
    assert!(e.project.relation(rel).is_ok());
    let refused = call(&mut e, "delete_box", json!({"box": "Web Shop"}));
    assert!(matches!(refused, Err(ApiError::Rejected(m)) if m.contains("recursive")));
    call(
        &mut e,
        "delete_box",
        json!({"box": "Web Shop", "recursive": true}),
    )?;
    assert_eq!(e.project.blocks.len(), 2);
    // Its relations went with it.
    assert!(e.project.relations.is_empty());
    let gone = call(&mut e, "delete_relation", json!({"relation": rel.0}));
    assert_eq!(gone, Err(ApiError::Rejected("unknown relation".into())));
    Ok(())
}

#[test]
fn texts_moves_and_relation_edits() -> TestResult {
    let mut e = Editor::new(None);
    let shop = shop(&mut e)?;
    call(&mut e, "set_motivation", json!({"text": "Who buys."}))?;
    call(
        &mut e,
        "set_motivation",
        json!({"diagram": shop, "text": "Split."}),
    )?;
    assert_eq!(e.project.motivation, "Who buys.");
    assert_eq!(e.project.motivation(Some(BlockId(shop))), "Split.");
    call(
        &mut e,
        "move_box",
        json!({"box": "Payment", "col": 1, "row": 2}),
    )?;
    let pay = e
        .project
        .blocks
        .values()
        .find(|b| b.name == "Payment")
        .ok_or("payment")?;
    assert_eq!(pay.cell, Cell::new(1, 2));
    let rel = *e.project.relations.keys().next().ok_or("rel")?;
    call(
        &mut e,
        "edit_relation",
        json!({"relation": rel.0, "direction": "bi"}),
    )?;
    let r = e.project.relation(rel)?;
    assert_eq!((r.direction, r.text.as_str()), (Direction::Bi, "orders"));
    assert!(
        e.last_ai
            .as_ref()
            .is_some_and(|a| a.summary == "edited a relation")
    );
    Ok(())
}

#[test]
fn render_returns_a_bounded_png() -> TestResult {
    let mut e = Editor::new(None);
    shop(&mut e)?;
    let Output::Png(png) = api::call(&mut e, "render_diagram", json!({}))? else {
        return Err("expected a png".into());
    };
    assert_eq!(&png[1..4], b"PNG");
    let w = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    assert!(w as f32 <= api::RENDER_LIMIT);
    Ok(())
}

#[test]
fn export_needs_an_absolute_folder_and_creates_it() -> TestResult {
    let mut e = Editor::new(None);
    shop(&mut e)?;
    let relative = call(
        &mut e,
        "export_docs",
        json!({"folder": "docs", "format": "markdown"}),
    );
    assert!(matches!(relative, Err(ApiError::Rejected(m)) if m.contains("absolute")));
    let dir = tempfile::tempdir()?;
    let target = dir.path().join("arc42/images");
    let out = call(
        &mut e,
        "export_docs",
        json!({"folder": target.to_string_lossy(), "format": "asciidoc"}),
    )?;
    assert_eq!(out["index"], "index.adoc");
    assert!(target.join("index.adoc").exists());
    assert!(target.join("context.svg").exists());
    Ok(())
}

#[test]
fn paths_whose_case_changes_byte_length_do_not_crash() -> TestResult {
    let mut e = Editor::new(None);
    call(&mut e, "add_box", json!({"name": "é", "kind": "component"}))?;
    call(
        &mut e,
        "add_box",
        json!({"diagram": "é", "name": "İİ", "kind": "component"}),
    )?;
    // 'İ' lowercases to two characters: lengths of the lowercased path differ.
    let found = call(
        &mut e,
        "set_responsibility",
        json!({"box": "é/İİ", "text": "x"}),
    )?;
    assert_eq!(found["path"], json!(["é", "İİ"]));
    let found = call(
        &mut e,
        "set_responsibility",
        json!({"box": "É/i̇i̇", "text": "y"}),
    )?;
    assert_eq!(found["path"], json!(["é", "İİ"]));
    Ok(())
}
