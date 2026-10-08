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
    assert_eq!(tools.len(), 18);
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
fn export_asks_once_per_folder_and_stays_inside_it() -> TestResult {
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
    let args = json!({"folder": target.to_string_lossy(), "format": "asciidoc"});
    // Not allowed yet: the user is asked, nothing is written.
    let asked = call(&mut e, "export_docs", args.clone());
    assert!(matches!(asked, Err(ApiError::Rejected(m)) if m.contains("asking them")));
    assert!(e.ai_export_request.is_some());
    assert!(!target.exists());
    // The user allows the repository folder; anything below it is fine.
    e.ai_export_request = None;
    e.allow_ai_export(dir.path())?;
    let out = call(&mut e, "export_docs", args)?;
    assert_eq!(out["index"], "index.adoc");
    assert!(target.join("index.adoc").exists());
    assert!(target.join("context.svg").exists());
    // `..` never resolves out of an allowed folder.
    let escape = format!("{}/arc42/../../outside", dir.path().display());
    let refused = call(
        &mut e,
        "export_docs",
        json!({"folder": escape, "format": "markdown"}),
    );
    assert!(matches!(refused, Err(ApiError::Rejected(m)) if m.contains("..")));
    // A sibling folder is a new question.
    let other = tempfile::tempdir()?;
    let sibling = call(
        &mut e,
        "export_docs",
        json!({"folder": other.path().to_string_lossy(), "format": "markdown"}),
    );
    assert!(sibling.is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn export_never_follows_symlinks_out_of_the_allowed_folder() -> TestResult {
    let mut e = Editor::new(None);
    shop(&mut e)?;
    let allowed = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    e.allow_ai_export(allowed.path())?;
    // A linked folder inside the allowed one points outside: not allowed.
    std::os::unix::fs::symlink(outside.path(), allowed.path().join("link"))?;
    let via_link = call(
        &mut e,
        "export_docs",
        json!({"folder": allowed.path().join("link").to_string_lossy(), "format": "markdown"}),
    );
    assert!(via_link.is_err());
    assert_eq!(std::fs::read_dir(outside.path())?.count(), 0);
    // A file that is a link is never written through.
    let victim = outside.path().join("victim.md");
    std::fs::write(&victim, "keep me")?;
    std::os::unix::fs::symlink(&victim, allowed.path().join("index.md"))?;
    let through = call(
        &mut e,
        "export_docs",
        json!({"folder": allowed.path().to_string_lossy(), "format": "markdown"}),
    );
    assert!(matches!(through, Err(ApiError::Rejected(m)) if m.contains("symbolic link")));
    assert_eq!(std::fs::read_to_string(&victim)?, "keep me");
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

#[test]
fn edit_relation_sets_and_clears_the_line_style() -> TestResult {
    use whiteboxed::model::LineStyle;
    let mut e = Editor::new(None);
    shop(&mut e)?;
    let rel = *e.project.relations.keys().next().ok_or("rel")?;
    let out = call(
        &mut e,
        "edit_relation",
        json!({"relation": rel.0, "line_style": "curved"}),
    )?;
    assert_eq!(out["line_style"], "curved");
    assert_eq!(e.project.relation(rel)?.style, Some(LineStyle::Curved));
    call(
        &mut e,
        "edit_relation",
        json!({"relation": rel.0, "line_style": "project_default"}),
    )?;
    assert_eq!(e.project.relation(rel)?.style, None);
    // Leaving it out keeps it.
    call(
        &mut e,
        "edit_relation",
        json!({"relation": rel.0, "line_style": "square"}),
    )?;
    call(
        &mut e,
        "edit_relation",
        json!({"relation": rel.0, "text": "x"}),
    )?;
    assert_eq!(e.project.relation(rel)?.style, Some(LineStyle::Square));
    Ok(())
}

#[test]
fn short_labels_come_with_the_relation_tools() -> TestResult {
    let mut e = Editor::new(None);
    call(&mut e, "add_box", json!({"name": "A", "kind": "component"}))?;
    let b = call(
        &mut e,
        "connect_new",
        json!({"from": "A", "side": "right", "name": "B", "kind": "component", "text": "a very long description of the call", "short_label": "calls"}),
    )?;
    let rel = b["relation"].as_u64().ok_or("relation")?;
    let r = e.project.relation(whiteboxed::model::RelationId(rel))?;
    assert_eq!(r.short, "calls");
    let out = call(
        &mut e,
        "edit_relation",
        json!({"relation": rel, "short_label": ""}),
    )?;
    assert_eq!(out["short_label"], "");
    let too_long = call(
        &mut e,
        "edit_relation",
        json!({"relation": rel, "short_label": "x".repeat(200)}),
    );
    assert!(too_long.is_err());
    Ok(())
}

#[test]
fn edit_relation_sets_the_side_at_a_box() -> TestResult {
    let mut e = Editor::new(None);
    shop(&mut e)?;
    let customer = e
        .project
        .blocks
        .iter()
        .find(|(_, b)| b.name == "Customer")
        .map(|(id, _)| *id)
        .ok_or("customer")?;
    let rel = *e
        .project
        .relations
        .iter()
        .find(|(_, r)| {
            r.a.anchors
                .iter()
                .chain(&r.b.anchors)
                .any(|a| a.block == customer)
        })
        .ok_or("relation")?
        .0;
    call(
        &mut e,
        "edit_relation",
        json!({"relation": rel.0, "sides": [{"box": "Customer", "side": "bottom"}]}),
    )?;
    let r = e.project.relation(rel)?;
    let side =
        r.a.anchors
            .iter()
            .chain(&r.b.anchors)
            .find(|a| a.block == customer)
            .map(|a| a.side);
    assert_eq!(side, Some(whiteboxed::model::Side::Bottom));
    // A box the relation does not touch is refused, and nothing changes.
    assert!(
        call(
            &mut e,
            "edit_relation",
            json!({"relation": rel.0, "text": "x", "sides": [{"box": "Payment", "side": "top"}]}),
        )
        .is_err()
    );
    assert_eq!(e.project.relation(rel)?.text, "orders");
    Ok(())
}

#[test]
fn relations_can_have_no_direction() -> TestResult {
    let mut e = Editor::new(None);
    shop(&mut e)?;
    let out = call(
        &mut e,
        "connect_new",
        json!({"from": "Web Shop", "side": "bottom", "name": "Ops", "kind": "person", "direction": "none", "text": "runbooks"}),
    )?;
    let rel = e
        .project
        .relations
        .values()
        .find(|r| r.text == "runbooks")
        .ok_or("relation")?;
    assert_eq!(rel.direction, Direction::Undirected);
    assert!(out.to_string().contains("Ops"));
    Ok(())
}

#[test]
fn add_box_can_make_a_band() -> TestResult {
    let mut e = Editor::new(None);
    shop(&mut e)?;
    let out = call(
        &mut e,
        "add_box",
        json!({"name": "Logging", "kind": "component", "band": true}),
    )?;
    assert_eq!(out["band"], true);
    assert!(out.get("cell").is_none());
    assert!(
        call(
            &mut e,
            "connect_existing",
            json!({"from": "Web Shop", "side": "bottom", "target": "Logging", "direction": "out"}),
        )
        .is_err()
    );
    let out = call(&mut e, "edit_box", json!({"box": "Logging", "band": false}))?;
    assert!(out.get("band").is_none());
    assert!(out.get("cell").is_some());
    Ok(())
}

#[test]
fn attach_fans_out_and_delete_relation_detaches_one_landing() -> TestResult {
    let mut e = Editor::new(None);
    shop(&mut e)?;
    for name in ["Storefront", "Checkout"] {
        call(
            &mut e,
            "add_box",
            json!({"diagram": "Web Shop", "name": name, "kind": "component"}),
        )?;
    }
    let rel = *e.project.relations.keys().next().ok_or("relation")?;
    for to in ["Web Shop/Storefront", "Web Shop/Checkout"] {
        call(
            &mut e,
            "attach",
            json!({"relation": rel.0, "whitebox": "Web Shop", "to": to}),
        )?;
    }
    let lines = |e: &mut Editor| -> Result<usize, ApiError> {
        let d = call(e, "get_diagram", json!({"diagram": "Web Shop"}))?;
        Ok(d["lines"]
            .as_array()
            .map_or(0, |l| l.iter().filter(|x| x["relation"] == rel.0).count()))
    };
    assert_eq!(lines(&mut e)?, 2);
    call(
        &mut e,
        "delete_relation",
        json!({"relation": rel.0, "detach_in": "Web Shop", "landing": "Web Shop/Checkout"}),
    )?;
    assert_eq!(lines(&mut e)?, 1);
    let refused = call(
        &mut e,
        "delete_relation",
        json!({"relation": rel.0, "landing": "Web Shop/Storefront"}),
    );
    assert!(matches!(refused, Err(ApiError::Rejected(_))));
    Ok(())
}

#[test]
fn batch_runs_all_steps_as_one_undo_step() -> TestResult {
    let mut e = Editor::new(None);
    shop(&mut e)?;
    let before = e.project.clone();
    let out = call(
        &mut e,
        "batch",
        json!({"steps": [
            {"tool": "add_box", "args": {"diagram": "Web Shop", "name": "Orders", "kind": "component"}},
            {"tool": "connect_new", "args": {"from": "Web Shop/Orders", "side": "bottom", "name": "Order DB", "kind": "database", "text": "SQL"}},
            {"tool": "set_responsibility", "args": {"box": "Web Shop/Orders", "text": "Takes orders."}}
        ]}),
    )?;
    assert_eq!(out["steps"].as_array().map(Vec::len), Some(3));
    assert!(e.project.blocks.values().any(|b| b.name == "Order DB"));
    assert!(
        e.last_ai
            .as_ref()
            .is_some_and(|a| a.summary.starts_with("3 changes"))
    );
    e.undo();
    assert_eq!(e.project, before, "one undo takes back the whole batch");
    Ok(())
}

#[test]
fn a_failing_step_changes_nothing_and_is_named() -> TestResult {
    let mut e = Editor::new(None);
    shop(&mut e)?;
    let before = e.project.clone();
    let failed = call(
        &mut e,
        "batch",
        json!({"steps": [
            {"tool": "add_box", "args": {"diagram": "Web Shop", "name": "Orders", "kind": "component"}},
            {"tool": "add_box", "args": {"diagram": "Web Shop", "name": "orders", "kind": "component"}}
        ]}),
    );
    assert!(
        matches!(&failed, Err(ApiError::Rejected(m)) if m.starts_with("step 2 (add_box) failed, nothing was changed")),
        "{failed:?}"
    );
    assert_eq!(e.project, before);
    // Undo still takes back the last call before the batch: connecting Payment.
    e.undo();
    assert!(!e.project.blocks.values().any(|b| b.name == "Payment"));
    assert!(
        e.project.blocks.is_empty() || e.project != before,
        "the undo stack is as before the batch"
    );
    for tool in ["batch", "export_docs", "render_diagram"] {
        let refused = call(
            &mut e,
            "batch",
            json!({"steps": [{"tool": tool, "args": {}}]}),
        );
        assert!(
            matches!(refused, Err(ApiError::Rejected(m)) if m.contains("cannot run in a batch"))
        );
    }
    Ok(())
}
