use whiteboxed::model::{
    BlockId, BlockKind, BlockSpec, Cell, Direction, End, ModelError, Placement, Project,
    RelationId, Side,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn component(name: &str) -> BlockSpec {
    BlockSpec::new(name, BlockKind::Component)
}

fn cell(p: &Project, id: BlockId) -> Result<Cell, ModelError> {
    Ok(p.block(id)?.cell)
}

fn sides(p: &Project, rel: RelationId) -> Result<(Side, Side), Box<dyn std::error::Error>> {
    let r = p.relation(rel)?;
    let a = r.a.anchors.first().ok_or("a open")?.side;
    let b = r.b.anchors.first().ok_or("b open")?.side;
    Ok((a, b))
}

#[test]
fn first_box_lands_in_the_origin_cell() -> TestResult {
    let mut p = Project::new();
    let shop = p.add_block(None, &component("Shop"))?;
    assert_eq!(cell(&p, shop)?, Cell::new(0, 0));
    assert_eq!(p.block(shop)?.parent, None);
    Ok(())
}

#[test]
fn names_are_unique_per_diagram_only() -> TestResult {
    let mut p = Project::new();
    let order = p.add_block(None, &component("Order"))?;
    let billing = p.add_block(None, &component("Billing"))?;
    assert_eq!(
        p.add_block(None, &component(" order ")),
        Err(ModelError::DuplicateName("order".into()))
    );
    p.add_block(Some(order), &component("Repository"))?;
    p.add_block(Some(billing), &component("Repository"))?;
    assert_eq!(
        p.add_block(None, &component("  ")),
        Err(ModelError::EmptyName)
    );
    Ok(())
}

#[test]
fn neighbours_only_exist_in_the_context_view_and_never_open() -> TestResult {
    let mut p = Project::new();
    let user = p.add_block(None, &BlockSpec::new("User", BlockKind::Person))?;
    let shop = p.add_block(None, &component("Shop"))?;
    assert_eq!(
        p.add_block(
            Some(shop),
            &BlockSpec::new("Payment", BlockKind::ExternalSystem)
        ),
        Err(ModelError::NeighbourBelowContext("external system"))
    );
    assert_eq!(
        p.add_block(Some(user), &component("Brain")),
        Err(ModelError::NotDrillable("person"))
    );
    let db = p.add_block(Some(shop), &BlockSpec::new("DB", BlockKind::Database))?;
    p.add_block(Some(db), &component("Shard"))?;
    assert_eq!(p.level(Some(db)), 2);
    Ok(())
}

#[test]
fn connect_new_places_the_partner_on_the_clicked_side() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let (b, rel) = p.connect_new(a, Side::Right, &component("B"), Direction::Out, " calls ")?;
    assert_eq!(cell(&p, b)?, Cell::new(1, 0));
    assert_eq!(sides(&p, rel)?, (Side::Right, Side::Left));
    assert_eq!(p.relation(rel)?.text, "calls");
    let (c, _) = p.connect_new(a, Side::Top, &component("C"), Direction::Out, "")?;
    assert_eq!(cell(&p, c)?, Cell::new(0, -1));
    Ok(())
}

#[test]
fn new_boxes_on_one_side_fill_a_square_block() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let mut cells = Vec::new();
    for i in 0..9 {
        let (b, _) = p.connect_new(
            a,
            Side::Right,
            &component(&format!("B{i}")),
            Direction::Out,
            "",
        )?;
        cells.push(cell(&p, b)?);
    }
    let c = |col, row| Cell::new(col, row);
    // 1, then 2 along the side, 2x2, 3x2, 3x3.
    assert_eq!(
        cells,
        vec![
            c(1, 0),
            c(1, 1),
            c(2, 0),
            c(2, 1),
            c(1, -1),
            c(2, -1),
            c(3, 0),
            c(3, 1),
            c(3, -1),
        ]
    );
    // Above A the block grows upward, around cells already taken: B4 holds (1, -1).
    let mut top = Vec::new();
    for i in 0..3 {
        let (t, _) = p.connect_new(
            a,
            Side::Top,
            &component(&format!("T{i}")),
            Direction::Out,
            "",
        )?;
        top.push(cell(&p, t)?);
    }
    assert_eq!(top, vec![c(0, -1), c(0, -2), c(1, -2)]);
    Ok(())
}

#[test]
fn connect_existing_moves_the_partner_to_the_clicked_side() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let (b, _) = p.connect_new(a, Side::Left, &component("B"), Direction::Out, "")?;
    let (c, _) = p.connect_new(a, Side::Bottom, &component("C"), Direction::Out, "")?;
    // C is below A; connecting C from A's right moves C to A's right.
    assert!(p.conflicts(a, Side::Right, c)?.len() == 1);
    let rel = p.connect_existing(a, Side::Right, c, Direction::Bi, "", Placement::Move)?;
    assert_eq!(cell(&p, c)?, Cell::new(1, 0));
    assert_eq!(sides(&p, rel)?, (Side::Right, Side::Left));
    // B was not involved and stays.
    assert_eq!(cell(&p, b)?, Cell::new(-1, 0));
    Ok(())
}

#[test]
fn a_partner_already_on_that_side_does_not_move() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let b = p.add_block(None, &component("B"))?;
    p.move_block(b, Cell::new(3, 2))?;
    assert!(p.conflicts(a, Side::Right, b)?.is_empty());
    let rel = p.connect_existing(a, Side::Right, b, Direction::Out, "", Placement::Move)?;
    assert_eq!(cell(&p, b)?, Cell::new(3, 2));
    assert_eq!(sides(&p, rel)?, (Side::Right, Side::Left));
    Ok(())
}

#[test]
fn moving_into_a_conflict_resides_the_old_relation() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let (b, ab) = p.connect_new(a, Side::Right, &component("B"), Direction::Out, "")?;
    let (c, bc) = p.connect_new(b, Side::Right, &component("C"), Direction::Out, "")?;
    // C at (2,0) connects B on its top: B moves above C. B->C (C right of B) breaks;
    // A->B still holds because B stays right of A.
    let conflicts = p.conflicts(c, Side::Top, b)?;
    assert_eq!(conflicts, vec![bc]);
    p.connect_existing(c, Side::Top, b, Direction::Out, "", Placement::Move)?;
    assert_eq!(cell(&p, b)?, Cell::new(2, -1));
    // B->C keeps its connection but now leaves B on the side facing C.
    assert_eq!(sides(&p, bc)?, (Side::Bottom, Side::Top));
    assert_eq!(sides(&p, ab)?, (Side::Right, Side::Left));
    Ok(())
}

#[test]
fn keep_leaves_the_partner_and_faces_it_back() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let (b, _) = p.connect_new(a, Side::Left, &component("B"), Direction::Out, "")?;
    let rel = p.connect_existing(a, Side::Right, b, Direction::Out, "", Placement::Keep)?;
    assert_eq!(cell(&p, b)?, Cell::new(-1, 0));
    assert_eq!(sides(&p, rel)?, (Side::Right, Side::Right));
    Ok(())
}

#[test]
fn connect_existing_rejects_self_and_other_diagrams() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let inner = p.add_block(Some(a), &component("Inner"))?;
    assert_eq!(
        p.connect_existing(a, Side::Right, a, Direction::Out, "", Placement::Move),
        Err(ModelError::SelfRelation)
    );
    assert_eq!(
        p.connect_existing(a, Side::Right, inner, Direction::Out, "", Placement::Move),
        Err(ModelError::DifferentDiagrams)
    );
    Ok(())
}

#[test]
fn move_block_swaps_with_the_occupant() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let b = p.add_block(None, &component("B"))?;
    p.move_block(a, Cell::new(1, 0))?;
    assert_eq!(cell(&p, a)?, Cell::new(1, 0));
    assert_eq!(cell(&p, b)?, Cell::new(0, 0));
    Ok(())
}

#[test]
fn stubs_bubble_up_to_the_context_view() -> TestResult {
    let mut p = Project::new();
    let shop = p.add_block(None, &component("Shop"))?;
    let order = p.add_block(Some(shop), &component("Order"))?;
    let repo = p.add_block(Some(order), &component("Repo"))?;
    let rel = p.add_stub(repo, Side::Bottom, Direction::Out, "events")?;
    let r = p.relation(rel)?;
    assert_eq!(r.owner, None);
    let chain: Vec<_> = r.a.anchors.iter().map(|a| (a.block, a.side)).collect();
    assert_eq!(
        chain,
        vec![
            (shop, Side::Bottom),
            (order, Side::Bottom),
            (repo, Side::Bottom)
        ]
    );
    assert!(r.b.is_open());
    Ok(())
}

#[test]
fn connecting_a_bubbled_stub_moves_it_into_that_diagram() -> TestResult {
    let mut p = Project::new();
    let shop = p.add_block(None, &component("Shop"))?;
    let order = p.add_block(Some(shop), &component("Order"))?;
    let bus = p.add_block(Some(shop), &BlockSpec::new("Bus", BlockKind::Queue))?;
    let repo = p.add_block(Some(order), &component("Repo"))?;
    let rel = p.add_stub(repo, Side::Right, Direction::Out, "events")?;
    assert_eq!(p.stub_anchor_in(rel, Some(shop))?.block, order);
    p.connect_stub(rel, Some(shop), bus, None, Placement::Move)?;
    let r = p.relation(rel)?;
    assert_eq!(r.owner, Some(shop));
    let near: Vec<_> = r.a.anchors.iter().map(|a| a.block).collect();
    assert_eq!(near, vec![order, repo]);
    assert_eq!(r.b.anchors.first().map(|a| a.block), Some(bus));
    // Bus moved to the right of Order.
    assert!(cell(&p, order)?.sees_on(Side::Right, cell(&p, bus)?));
    Ok(())
}

#[test]
fn attach_and_remove_line_inside_a_whitebox() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let (b, rel) = p.connect_new(a, Side::Right, &component("B"), Direction::Out, "")?;
    let b1 = p.add_block(Some(b), &component("B1"))?;
    p.attach(rel, End::B, b, b1, Side::Left)?;
    let blocks: Vec<_> = p.relation(rel)?.b.anchors.iter().map(|x| x.block).collect();
    assert_eq!(blocks, vec![b, b1]);
    // Deleting the line inside B only detaches; A->B stays.
    p.remove_line(rel, Some(b))?;
    let blocks: Vec<_> = p.relation(rel)?.b.anchors.iter().map(|x| x.block).collect();
    assert_eq!(blocks, vec![b]);
    // Deleting it on its own level removes it.
    p.remove_line(rel, None)?;
    assert!(p.relation(rel).is_err());
    Ok(())
}

#[test]
fn attach_requires_a_box_inside_the_whitebox() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let (b, rel) = p.connect_new(a, Side::Right, &component("B"), Direction::Out, "")?;
    let a1 = p.add_block(Some(a), &component("A1"))?;
    assert_eq!(
        p.attach(rel, End::B, b, a1, Side::Left),
        Err(ModelError::DifferentDiagrams)
    );
    assert_eq!(
        p.attach(rel, End::B, a, a1, Side::Left),
        Err(ModelError::NotOnChain)
    );
    Ok(())
}

#[test]
fn deleting_a_box_removes_content_and_relations_but_detaches_inherited_ends() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let (b, ab) = p.connect_new(a, Side::Right, &component("B"), Direction::Out, "")?;
    let b1 = p.add_block(Some(b), &component("B1"))?;
    let (b2, inner) = p.connect_new(b1, Side::Right, &component("B2"), Direction::Out, "")?;
    p.add_block(Some(b1), &component("Deep"))?;
    p.attach(ab, End::B, b, b1, Side::Left)?;
    p.delete_block(b1)?;
    assert!(p.relation(inner).is_err());
    assert_eq!(p.blocks.len(), 3); // A, B, B2
    assert!(p.block(b2).is_ok());
    let blocks: Vec<_> = p.relation(ab)?.b.anchors.iter().map(|x| x.block).collect();
    assert_eq!(blocks, vec![b]);
    p.delete_block(b)?;
    assert!(p.relation(ab).is_err());
    assert_eq!(p.blocks.len(), 1);
    Ok(())
}

#[test]
fn editing_a_box_validates_like_creating_it() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    p.add_block(None, &component("B"))?;
    p.add_block(Some(a), &component("A1"))?;
    assert_eq!(
        p.edit_block(a, &component("b")),
        Err(ModelError::DuplicateName("b".into()))
    );
    assert_eq!(
        p.edit_block(a, &BlockSpec::new("A", BlockKind::Person)),
        Err(ModelError::KindHasContent("person"))
    );
    p.edit_block(
        a,
        &BlockSpec::new("Alpha", BlockKind::Database).tagged("legacy"),
    )?;
    let block = p.block(a)?;
    assert_eq!(block.name, "Alpha");
    assert_eq!(block.kind, BlockKind::Database);
    assert_eq!(block.tag, p.tag_by_name("Legacy"));
    Ok(())
}

#[test]
fn tags_are_project_wide_with_palette_colours() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A").tagged("legacy"))?;
    let inner = p.add_block(Some(a), &component("Inner").tagged("LEGACY"))?;
    let other = p.add_block(None, &component("B").tagged("new"))?;
    assert_eq!(p.tags.len(), 2);
    assert_eq!(p.block(a)?.tag, p.block(inner)?.tag);
    assert_ne!(p.block(a)?.tag, p.block(other)?.tag);
    let blank = p.add_block(None, &component("C").tagged("  "))?;
    assert_eq!(p.block(blank)?.tag, None);
    Ok(())
}

#[test]
fn responsibility_and_motivation_are_stored_per_box_and_diagram() -> TestResult {
    let mut p = Project::new();
    let shop = p.add_block(None, &component("Shop"))?;
    let user = p.add_block(None, &BlockSpec::new("User", BlockKind::Person))?;
    p.set_responsibility(shop, "  Sells things.\n")?;
    p.set_motivation(None, "The shop and who uses it.")?;
    p.set_motivation(Some(shop), "Split by business capability.")?;
    assert_eq!(p.block(shop)?.responsibility, "Sells things.");
    assert_eq!(p.motivation(None), "The shop and who uses it.");
    assert_eq!(p.motivation(Some(shop)), "Split by business capability.");
    assert_eq!(
        p.set_motivation(Some(user), "x"),
        Err(ModelError::NotDrillable("person"))
    );
    // Editing name, type and tag keeps the texts.
    p.edit_block(shop, &component("Web Shop"))?;
    assert_eq!(p.block(shop)?.responsibility, "Sells things.");
    Ok(())
}

#[test]
fn limits_keep_the_project_drawable() -> TestResult {
    use whiteboxed::model::{MAX_BLOCKS_PER_DIAGRAM, MAX_CELL, MAX_NAME, MAX_TEXT};
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    assert_eq!(
        p.add_block(None, &component(&"x".repeat(MAX_NAME + 1))),
        Err(ModelError::NameTooLong)
    );
    assert_eq!(
        p.add_block(None, &component("bad\u{7}name")),
        Err(ModelError::ControlCharacter)
    );
    assert_eq!(
        p.add_block(None, &component("B").tagged("tag\u{0}")),
        Err(ModelError::ControlCharacter)
    );
    assert_eq!(
        p.set_responsibility(a, &"y".repeat(MAX_TEXT + 1)),
        Err(ModelError::TextTooLong)
    );
    p.set_responsibility(a, "line one\r\nline two\tend")?;
    assert_eq!(p.block(a)?.responsibility, "line one\nline two\tend");
    assert_eq!(
        p.move_block(a, Cell::new(MAX_CELL + 1, 0)),
        Err(ModelError::OutsideGrid)
    );
    assert_eq!(
        p.move_block(a, Cell::new(i32::MAX, 0)),
        Err(ModelError::OutsideGrid)
    );
    // A box at the edge cannot get a neighbour beyond it.
    p.move_block(a, Cell::new(MAX_CELL, 0))?;
    assert_eq!(
        p.connect_new(a, Side::Right, &component("Beyond"), Direction::Out, ""),
        Err(ModelError::OutsideGrid)
    );
    let mut q = Project::new();
    for i in 0..MAX_BLOCKS_PER_DIAGRAM {
        q.add_block(None, &component(&format!("B{i}")))?;
    }
    assert_eq!(
        q.add_block(None, &component("one too many")),
        Err(ModelError::DiagramFull)
    );
    Ok(())
}

#[test]
fn a_diagram_holds_a_bounded_number_of_relations() -> TestResult {
    use whiteboxed::model::MAX_RELATIONS_PER_DIAGRAM;
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let b = p.add_block(None, &component("B"))?;
    for _ in 0..MAX_RELATIONS_PER_DIAGRAM {
        p.connect_existing(a, Side::Right, b, Direction::Out, "", Placement::Keep)?;
    }
    assert_eq!(
        p.connect_existing(a, Side::Right, b, Direction::Out, "", Placement::Keep),
        Err(ModelError::TooManyRelations)
    );
    assert_eq!(
        p.add_stub(a, Side::Top, Direction::Out, ""),
        Err(ModelError::TooManyRelations)
    );
    Ok(())
}

#[test]
fn moving_a_box_turns_its_lines_to_face_the_partner() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let (b, ab) = p.connect_new(a, Side::Right, &component("B"), Direction::Out, "")?;
    p.move_block(b, Cell::new(0, 2))?;
    assert_eq!(sides(&p, ab)?, (Side::Bottom, Side::Top));
    p.move_block(a, Cell::new(-2, 2))?;
    assert_eq!(sides(&p, ab)?, (Side::Right, Side::Left));
    Ok(())
}

#[test]
fn a_box_swapped_away_faces_its_partners_too() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let (b, ab) = p.connect_new(a, Side::Right, &component("B"), Direction::Out, "")?;
    let (_, bc) = p.connect_new(b, Side::Bottom, &component("C"), Direction::Out, "")?;
    // A takes B's cell, so B moves to A's old cell, left of A and of C's column.
    p.move_block(a, Cell::new(1, 0))?;
    assert_eq!(cell(&p, b)?, Cell::new(0, 0));
    assert_eq!(sides(&p, ab)?, (Side::Left, Side::Right));
    assert_eq!(sides(&p, bc)?, (Side::Right, Side::Left));
    Ok(())
}

#[test]
fn a_side_set_by_hand_holds_until_the_next_move() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let (b, ab) = p.connect_new(a, Side::Right, &component("B"), Direction::Out, "")?;
    p.set_side_at(ab, a, Side::Top)?;
    p.set_side_at(ab, b, Side::Top)?;
    assert_eq!(sides(&p, ab)?, (Side::Top, Side::Top));
    let c = p.add_block(None, &component("C"))?;
    assert_eq!(p.set_side_at(ab, c, Side::Top), Err(ModelError::NotOnChain));
    p.move_block(b, Cell::new(1, 1))?;
    assert_eq!(sides(&p, ab)?, (Side::Right, Side::Left));
    Ok(())
}

#[test]
fn moving_an_inner_box_faces_the_frame_its_line_enters_at() -> TestResult {
    let mut p = Project::new();
    let shop = p.add_block(None, &component("Shop"))?;
    let (_, rel) = p.connect_new(shop, Side::Left, &component("Customer"), Direction::In, "")?;
    let inner = p.add_block(Some(shop), &component("UI"))?;
    p.attach(rel, End::A, shop, inner, Side::Bottom)?;
    p.move_block(inner, Cell::new(2, 1))?;
    let r = p.relation(rel)?;
    let chain: Vec<(BlockId, Side)> = r.a.anchors.iter().map(|a| (a.block, a.side)).collect();
    assert_eq!(chain, vec![(shop, Side::Left), (inner, Side::Left)]);
    Ok(())
}

#[test]
fn add_block_at_takes_the_cell_or_the_next_free_one() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block_at(None, &component("A"), Cell::new(2, 3))?;
    assert_eq!(cell(&p, a)?, Cell::new(2, 3));
    let b = p.add_block_at(None, &component("B"), Cell::new(2, 3))?;
    // Taken: the nearest free cell next to it.
    assert_eq!(cell(&p, b)?, Cell::new(2, 4));
    assert_eq!(
        p.add_block_at(
            None,
            &component("C"),
            Cell::new(whiteboxed::model::MAX_CELL + 1, 0)
        ),
        Err(ModelError::OutsideGrid)
    );
    Ok(())
}

#[test]
fn the_project_is_named_after_its_system_until_named_by_hand() -> TestResult {
    let mut p = Project::new();
    assert_eq!(p.display_name(), None);
    let user = p.add_block(None, &BlockSpec::new("Customer", BlockKind::Person))?;
    // People and external systems do not name the project.
    assert_eq!(p.display_name(), None);
    let (shop, _) = p.connect_new(
        user,
        Side::Right,
        &component("Web Shop"),
        Direction::Out,
        "",
    )?;
    p.connect_new(shop, Side::Right, &component("Billing"), Direction::Out, "")?;
    assert_eq!(p.display_name().as_deref(), Some("Web Shop"));
    p.edit_block(shop, &component("Shop"))?;
    assert_eq!(p.display_name().as_deref(), Some("Shop"));
    p.set_name("  Sanshain ")?;
    assert_eq!(p.display_name().as_deref(), Some("Sanshain"));
    p.edit_block(shop, &component("Store"))?;
    assert_eq!(p.display_name().as_deref(), Some("Sanshain"));
    assert_eq!(p.set_name("a\nb"), Err(ModelError::ControlCharacter));
    p.set_name("")?;
    assert_eq!(p.display_name().as_deref(), Some("Store"));
    Ok(())
}
