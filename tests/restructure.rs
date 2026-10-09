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
