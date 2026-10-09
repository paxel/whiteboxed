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
