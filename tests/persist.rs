use whiteboxed::model::{BlockKind, BlockSpec, Direction, End, Project, Side};
use whiteboxed::persist::{self, PersistError};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn sample() -> Result<Project, Box<dyn std::error::Error>> {
    let mut p = Project::new();
    let user = p.add_block(None, &BlockSpec::new("User", BlockKind::Person))?;
    let (shop, _) = p.connect_new(
        user,
        Side::Right,
        &BlockSpec::new("Shop", BlockKind::Component).tagged("core"),
        Direction::Out,
        "buys",
    )?;
    let order = p.add_block(Some(shop), &BlockSpec::new("Order", BlockKind::Component))?;
    p.add_stub(order, Side::Bottom, Direction::Out, "")?;
    let rel = *p.relations.keys().next().ok_or("relation")?;
    p.attach(rel, End::B, shop, order, Side::Left)?;
    Ok(p)
}

const GOLDEN: &str = "\
format: 1
tags:
- id: 2
  name: core
  color: '#a8d5f7'
blocks:
- id: 1
  name: User
  kind: person
  cell:
    col: 0
    row: 0
- id: 3
  name: Shop
  kind: component
  tag: 2
  cell:
    col: 1
    row: 0
- id: 5
  name: Order
  kind: component
  parent: 3
  cell:
    col: 0
    row: 0
relations:
- id: 4
  a:
  - block: 1
    side: right
  b:
  - block: 3
    side: left
  - block: 5
    side: left
  direction: out
  text: buys
- id: 6
  a:
  - block: 3
    side: bottom
  - block: 5
    side: bottom
  b: []
  direction: out
";

#[test]
fn yaml_is_stable_and_readable() -> TestResult {
    assert_eq!(persist::to_yaml(&sample()?)?, GOLDEN);
    Ok(())
}

#[test]
fn round_trip_keeps_everything() -> TestResult {
    let p = sample()?;
    let back = persist::from_yaml(&persist::to_yaml(&p)?)?;
    assert_eq!(back, p);
    Ok(())
}

#[test]
fn save_and_load_through_a_file() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("shop.yaml");
    let p = sample()?;
    persist::save(&path, &p)?;
    persist::save(&path, &p)?;
    assert_eq!(persist::load(&path)?, p);
    let leftovers = std::fs::read_dir(dir.path())?.count();
    assert_eq!(leftovers, 1);
    Ok(())
}

#[test]
fn loaded_ids_continue_after_the_highest() -> TestResult {
    let mut p = persist::from_yaml(GOLDEN)?;
    let id = p.add_block(None, &BlockSpec::new("New", BlockKind::Component))?;
    assert_eq!(id.0, 7);
    Ok(())
}

#[test]
fn broken_references_are_rejected() {
    let broken = GOLDEN.replace("parent: 3", "parent: 99");
    assert!(matches!(
        persist::from_yaml(&broken),
        Err(PersistError::Invalid(_))
    ));
    let skipped = GOLDEN.replace("  - block: 5\n    side: left\n", "");
    assert!(persist::from_yaml(&skipped).is_ok());
    let wrong_level = GOLDEN.replace(
        "  - block: 1\n    side: right\n",
        "  - block: 5\n    side: right\n",
    );
    assert!(matches!(
        persist::from_yaml(&wrong_level),
        Err(PersistError::Invalid(_))
    ));
    assert!(matches!(
        persist::from_yaml("format: 1\nblocks: nope\n"),
        Err(PersistError::Yaml(_))
    ));
}

#[test]
fn texts_survive_the_file_and_stay_out_when_empty() -> TestResult {
    let mut p = sample()?;
    let shop = p
        .blocks
        .iter()
        .find(|(_, b)| b.name == "Shop")
        .map(|(id, _)| *id)
        .ok_or("shop")?;
    p.set_responsibility(shop, "Sells things.\nShips them, too.")?;
    p.set_motivation(None, "Who buys: customers.")?;
    p.set_motivation(Some(shop), "One box per capability.")?;
    let yaml = persist::to_yaml(&p)?;
    assert!(yaml.contains("motivation:"));
    assert_eq!(persist::from_yaml(&yaml)?, p);
    // Projects without texts keep the old, unchanged file.
    assert!(!GOLDEN.contains("responsibility"));
    assert_eq!(persist::to_yaml(&sample()?)?, GOLDEN);
    Ok(())
}

#[test]
fn line_styles_are_saved_only_when_not_default() -> TestResult {
    use whiteboxed::model::LineStyle;
    let mut p = sample()?;
    assert!(!persist::to_yaml(&p)?.contains("line_style"));
    let rel = *p.relations.keys().next().ok_or("relation")?;
    p.set_relation_style(rel, Some(LineStyle::Curved))?;
    p.set_line_style(LineStyle::Square);
    let yaml = persist::to_yaml(&p)?;
    assert!(yaml.contains("line_style: square"));
    assert!(yaml.contains("style: curved"));
    assert_eq!(persist::from_yaml(&yaml)?, p);
    Ok(())
}

#[test]
fn label_limits_and_short_labels_survive_the_file() -> TestResult {
    let mut p = sample()?;
    assert!(!persist::to_yaml(&p)?.contains("label_limit"));
    let rel = *p.relations.keys().next().ok_or("relation")?;
    p.set_relation_short(rel, "web")?;
    p.set_label_limit(Some(10));
    let yaml = persist::to_yaml(&p)?;
    assert!(yaml.contains("label_limit: 10"));
    assert!(yaml.contains("short: web"));
    assert_eq!(persist::from_yaml(&yaml)?, p);
    p.set_label_limit(None);
    let yaml = persist::to_yaml(&p)?;
    assert!(yaml.contains("label_limit: never"));
    assert_eq!(persist::from_yaml(&yaml)?, p);
    Ok(())
}
