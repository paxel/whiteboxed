use whiteboxed::model::{
    BlockId, BlockKind, BlockSpec, Direction, End, ModelError, Project, RelationId, Side,
};
use whiteboxed::view::{ViewEnd, dangling_count, diagram_view};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn component(name: &str) -> BlockSpec {
    BlockSpec::new(name, BlockKind::Component)
}

/// The blocks of an end, in order.
fn chain(p: &Project, rel: RelationId, end: End) -> Result<Vec<BlockId>, ModelError> {
    Ok(p.relation(rel)?
        .end(end)
        .anchors
        .iter()
        .map(|a| a.block)
        .collect())
}

/// Customer uses Business; inside Business, Logic uses an embedded DB below it.
struct Shop {
    p: Project,
    customer: BlockId,
    business: BlockId,
    logic: BlockId,
    db: BlockId,
    uses: RelationId,
    sql: RelationId,
}

fn shop() -> Result<Shop, Box<dyn std::error::Error>> {
    let mut p = Project::new();
    let customer = p.add_block(None, &BlockSpec::new("Customer", BlockKind::Person))?;
    let (business, uses) = p.connect_new(
        customer,
        Side::Right,
        &component("Business"),
        Direction::Out,
        "uses",
    )?;
    let logic = p.add_block(Some(business), &component("Logic"))?;
    let (db, sql) = p.connect_new(
        logic,
        Side::Bottom,
        &BlockSpec::new("DB", BlockKind::Database),
        Direction::Out,
        "SQL",
    )?;
    p.attach(uses, End::B, business, logic, Side::Left)?;
    Ok(Shop {
        p,
        customer,
        business,
        logic,
        db,
        uses,
        sql,
    })
}

#[test]
fn a_box_moved_up_keeps_its_lines() -> TestResult {
    let Shop {
        mut p,
        business,
        logic,
        db,
        sql,
        ..
    } = shop()?;
    p.move_up(db)?;
    let moved = p.block(db)?;
    assert_eq!(moved.parent, None);
    // Logic used it from above, so it comes out below Business.
    assert_eq!(moved.cell, p.block(business)?.cell.toward(Side::Bottom));
    // The SQL line now belongs to the context view: Business (Logic inside) to DB.
    let r = p.relation(sql)?;
    assert_eq!(r.owner, None);
    assert_eq!(chain(&p, sql, End::A)?, vec![business, logic]);
    assert_eq!(chain(&p, sql, End::B)?, vec![db]);
    assert_eq!(r.a.anchors[0].side, Side::Bottom);
    assert_eq!(r.b.anchors[0].side, Side::Top);
    // Inside Business it enters through the frame and lands on Logic.
    let inside = diagram_view(&p, Some(business));
    assert_eq!(dangling_count(&inside), 0);
    // Still a valid project file.
    whiteboxed::persist::from_yaml(&whiteboxed::persist::to_yaml(&p)?)?;
    assert!(inside.lines.iter().any(|l| l.relation == sql
        && matches!(l.b, ViewEnd::Frame { ref partner, .. } if partner == "DB")));
    Ok(())
}

#[test]
fn a_line_that_landed_on_the_moved_box_and_another_splits() -> TestResult {
    let Shop {
        mut p,
        customer,
        business,
        logic,
        db,
        uses,
        ..
    } = shop()?;
    p.attach(uses, End::B, business, db, Side::Left)?;
    p.move_up(db)?;
    // The customer's line still lands on Logic inside Business ...
    assert_eq!(chain(&p, uses, End::B)?, vec![business, logic]);
    // ... and a second relation with the same text now goes straight to the DB.
    let to_db: Vec<_> = p
        .relations
        .iter()
        .filter(|(id, r)| **id != uses && r.text == "uses")
        .collect();
    assert_eq!(to_db.len(), 1);
    let (_, r) = to_db[0];
    assert_eq!(r.owner, None);
    assert_eq!(
        r.a.anchors.iter().map(|a| a.block).collect::<Vec<_>>(),
        vec![customer]
    );
    assert_eq!(
        r.b.anchors.iter().map(|a| a.block).collect::<Vec<_>>(),
        vec![db]
    );
    Ok(())
}

#[test]
fn moving_up_keeps_what_lies_inside_and_refuses_what_it_cannot() -> TestResult {
    let Shop {
        mut p,
        business,
        logic,
        db,
        ..
    } = shop()?;
    // The DB has a whitebox of its own with a relation inside: it moves along.
    let tables = p.add_block(Some(db), &component("Tables"))?;
    p.connect_new(tables, Side::Right, &component("Index"), Direction::Out, "")?;
    p.move_up(db)?;
    assert_eq!(p.block(tables)?.parent, Some(db));
    assert_eq!(p.move_up(business), Err(ModelError::AtTop));
    // A name that is taken one level up.
    p.add_block(None, &component("Logic"))?;
    assert_eq!(
        p.move_up(logic),
        Err(ModelError::DuplicateName("Logic".into()))
    );
    assert_eq!(p.block(logic)?.parent, Some(business));
    Ok(())
}

#[test]
fn moving_into_a_neighbour_is_the_reverse_of_moving_up() -> TestResult {
    let Shop {
        mut p,
        business,
        logic,
        db,
        sql,
        ..
    } = shop()?;
    let health = p.add_block(None, &component("Health"))?;
    p.move_up(db)?;
    let check = p.connect_existing(
        health,
        Side::Right,
        db,
        Direction::Out,
        "check",
        whiteboxed::model::Placement::Keep,
    )?;
    p.move_into(db, business)?;
    assert_eq!(p.block(db)?.parent, Some(business));
    // Next to Logic, on the side it came from (below).
    assert_eq!(p.block(db)?.cell, p.block(logic)?.cell.toward(Side::Bottom));
    // SQL is internal to Business again.
    assert_eq!(p.relation(sql)?.owner, Some(business));
    assert_eq!(chain(&p, sql, End::A)?, vec![logic]);
    assert_eq!(chain(&p, sql, End::B)?, vec![db]);
    // Health's check now enters Business and lands on the DB.
    assert_eq!(p.relation(check)?.owner, None);
    assert_eq!(chain(&p, check, End::B)?, vec![business, db]);
    assert_eq!(dangling_count(&diagram_view(&p, Some(business))), 0);
    whiteboxed::persist::from_yaml(&whiteboxed::persist::to_yaml(&p)?)?;
    Ok(())
}

#[test]
fn a_line_to_the_target_that_reached_nothing_inside_becomes_a_stub() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &component("A"))?;
    let (b, rel) = p.connect_new(a, Side::Right, &component("B"), Direction::Out, "calls")?;
    p.add_block(Some(b), &component("Inner"))?;
    p.move_into(a, b)?;
    let r = p.relation(rel)?;
    // A sits inside B now; the line had no box to reach in B, so A keeps an open end.
    assert!(r.b.is_open() || r.a.is_open());
    assert_eq!(p.block(a)?.parent, Some(b));
    whiteboxed::persist::from_yaml(&whiteboxed::persist::to_yaml(&p)?)?;
    Ok(())
}

#[test]
fn move_into_refuses_what_cannot_go_there() -> TestResult {
    let Shop {
        mut p,
        customer,
        business,
        ..
    } = shop()?;
    let ext = p.add_block(None, &BlockSpec::new("Bank", BlockKind::ExternalSystem))?;
    assert_eq!(
        p.move_into(business, ext),
        Err(ModelError::NotDrillable("external system"))
    );
    assert_eq!(
        p.move_into(customer, business),
        Err(ModelError::NeighbourBelowContext("person"))
    );
    assert_eq!(
        p.move_into(business, business),
        Err(ModelError::SelfRelation)
    );
    Ok(())
}

#[test]
fn grouping_puts_boxes_into_a_new_whitebox() -> TestResult {
    let mut p = Project::new();
    let customer = p.add_block(None, &BlockSpec::new("Customer", BlockKind::Person))?;
    let (a, buys) = p.connect_new(
        customer,
        Side::Right,
        &component("A"),
        Direction::Out,
        "buys",
    )?;
    let (b, ab) = p.connect_new(a, Side::Right, &component("B"), Direction::Out, "")?;
    let (c, bc) = p.connect_new(b, Side::Right, &component("C"), Direction::Out, "")?;
    let (a_cell, b_cell) = (p.block(a)?.cell, p.block(b)?.cell);
    let core = p.group(&[a, b], a, &component("Core"))?;
    assert_eq!(p.block(core)?.cell, a_cell);
    assert_eq!(p.block(a)?.parent, Some(core));
    assert_eq!((p.block(a)?.cell, p.block(b)?.cell), (a_cell, b_cell));
    // Between the grouped boxes: inside the new whitebox.
    assert_eq!(p.relation(ab)?.owner, Some(core));
    // From outside: to the new box, landing on the box inside.
    assert_eq!(chain(&p, buys, End::B)?, vec![core, a]);
    assert_eq!(chain(&p, bc, End::A)?, vec![core, b]);
    assert_eq!(chain(&p, bc, End::B)?, vec![c]);
    assert_eq!(dangling_count(&diagram_view(&p, Some(core))), 0);
    whiteboxed::persist::from_yaml(&whiteboxed::persist::to_yaml(&p)?)?;
    // Grouping a person, or at a box outside the group, is refused.
    assert_eq!(
        p.group(&[customer], customer, &component("People")),
        Err(ModelError::NeighbourBelowContext("person"))
    );
    assert_eq!(
        p.group(&[c], core, &component("X")),
        Err(ModelError::NotInGroup)
    );
    Ok(())
}

#[test]
fn dissolving_lets_the_inner_boxes_take_the_place_of_the_box() -> TestResult {
    let Shop {
        mut p,
        customer,
        business,
        logic,
        db,
        uses,
        sql,
    } = shop()?;
    // Inside, Cache sits right of Logic: the inner block is two columns wide.
    p.connect_new(logic, Side::Right, &component("Cache"), Direction::Out, "")?;
    let right = p.add_block(None, &component("Right"))?;
    p.move_block(right, p.block(business)?.cell.toward(Side::Right))?;
    let health = p.add_block(None, &component("Health"))?;
    let (bcell, rcell) = (p.block(business)?.cell, p.block(right)?.cell);
    let probe = p.connect_existing(
        health,
        Side::Top,
        business,
        Direction::Out,
        "probe",
        whiteboxed::model::Placement::Keep,
    )?;
    p.dissolve(business)?;
    assert!(p.block(business).is_err());
    // Logic (top left inside) takes the old cell, the DB sits below it.
    assert_eq!(p.block(logic)?.cell, bcell);
    assert_eq!(p.block(logic)?.parent, None);
    assert_eq!(p.block(db)?.cell, bcell.toward(Side::Bottom));
    // The box on the right moved aside by one column.
    assert_eq!(p.block(right)?.cell, rcell.toward(Side::Right));
    // The customer now uses Logic directly, and SQL is a context relation.
    assert_eq!(chain(&p, uses, End::A)?, vec![customer]);
    assert_eq!(chain(&p, uses, End::B)?, vec![logic]);
    assert_eq!(p.relation(sql)?.owner, None);
    // Health's probe reached nothing inside: it is an open end at Health now.
    let r = p.relation(probe)?;
    assert!(r.b.is_open());
    assert_eq!(chain(&p, probe, End::A)?, vec![health]);
    whiteboxed::persist::from_yaml(&whiteboxed::persist::to_yaml(&p)?)?;
    Ok(())
}

#[test]
fn dissolving_splits_a_line_that_fanned_out() -> TestResult {
    let Shop {
        mut p,
        business,
        db,
        uses,
        ..
    } = shop()?;
    p.attach(uses, End::B, business, db, Side::Left)?;
    let before = p.relations.len();
    p.dissolve(business)?;
    assert_eq!(p.relations.len(), before + 1);
    let to: Vec<Vec<BlockId>> = p
        .relations
        .values()
        .filter(|r| r.text == "uses")
        .map(|r| r.b.anchors.iter().map(|a| a.block).collect())
        .collect();
    assert_eq!(to.len(), 2);
    assert!(to.contains(&vec![db]));
    Ok(())
}

#[test]
fn dissolving_refuses_names_that_are_taken_above() -> TestResult {
    let Shop {
        mut p, business, ..
    } = shop()?;
    p.add_block(None, &component("Logic"))?;
    assert_eq!(
        p.dissolve(business),
        Err(ModelError::DuplicateName("Logic".into()))
    );
    Ok(())
}

#[test]
fn splitting_a_relation_makes_one_per_pair_of_boxes() -> TestResult {
    use whiteboxed::model::SplitPart;
    let mut p = Project::new();
    let s = p.add_block(None, &component("S"))?;
    let a = p.add_block(Some(s), &component("A"))?;
    let (b, rel) = p.connect_new(a, Side::Right, &component("B"), Direction::Bi, "talks")?;
    let inside = |p: &mut Project, outer, name: &str, end, side| -> Result<BlockId, ModelError> {
        let id = p.add_block(Some(outer), &component(name))?;
        p.attach(rel, end, outer, id, side)?;
        Ok(id)
    };
    let a1 = inside(&mut p, a, "a1", End::A, Side::Right)?;
    let a2 = inside(&mut p, a, "a2", End::A, Side::Right)?;
    let b1 = inside(&mut p, b, "b1", End::B, Side::Left)?;
    let b2 = inside(&mut p, b, "b2", End::B, Side::Left)?;
    assert_eq!(p.split_choices(rel, End::A)?, vec![a1, a2]);
    let ids = p.split_relation(
        rel,
        &[
            SplitPart {
                a: a1,
                b: b1,
                text: "provide".into(),
                direction: Direction::Out,
            },
            SplitPart {
                a: a2,
                b: b2,
                text: "require".into(),
                direction: Direction::In,
            },
        ],
    )?;
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], rel);
    assert_eq!(chain(&p, rel, End::A)?, vec![a, a1]);
    assert_eq!(chain(&p, rel, End::B)?, vec![b, b1]);
    assert_eq!(chain(&p, ids[1], End::A)?, vec![a, a2]);
    assert_eq!(chain(&p, ids[1], End::B)?, vec![b, b2]);
    assert_eq!(p.relation(ids[1])?.direction, Direction::In);
    // On the level of A and B they are still one line, with both texts.
    let l = whiteboxed::layout::layout(&p, &diagram_view(&p, Some(s)));
    assert_eq!(l.lines.len(), 1);
    assert_eq!(l.lines[0].text, "provide, require");
    // Inside A every box sees exactly its partner.
    let view = diagram_view(&p, Some(a));
    let partners: Vec<String> = view
        .lines
        .iter()
        .filter_map(|l| match &l.b {
            ViewEnd::Frame { partner, .. } => Some(partner.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(partners, vec!["B \u{203a} b1", "B \u{203a} b2"]);
    whiteboxed::persist::from_yaml(&whiteboxed::persist::to_yaml(&p)?)?;
    Ok(())
}

#[test]
fn splitting_refuses_open_ends_and_strangers() -> TestResult {
    use whiteboxed::model::SplitPart;
    let Shop {
        mut p,
        customer,
        business,
        logic,
        uses,
        ..
    } = shop()?;
    let part = |a, b| SplitPart {
        a,
        b,
        text: String::new(),
        direction: Direction::Out,
    };
    // End B of "uses" lands on Logic inside Business: Business itself is no choice.
    assert_eq!(
        p.split_relation(uses, &[part(customer, business)]),
        Err(ModelError::NotOnChain)
    );
    assert_eq!(p.split_relation(uses, &[]), Err(ModelError::SplitEmpty));
    p.split_relation(uses, &[part(customer, logic)])?;
    let stub = p.add_stub(business, Side::Top, Direction::Out, "metrics")?;
    assert_eq!(
        p.split_relation(stub, &[part(business, business)]),
        Err(ModelError::SplitOpen)
    );
    Ok(())
}
