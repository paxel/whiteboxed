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

#[test]
fn the_project_name_is_saved_only_when_set() -> TestResult {
    let mut p = sample()?;
    assert!(!persist::to_yaml(&p)?.contains("name: Sanshain"));
    p.set_name("Sanshain")?;
    let yaml = persist::to_yaml(&p)?;
    assert!(yaml.starts_with("format: 1\nname: Sanshain\n"), "{yaml}");
    assert_eq!(persist::from_yaml(&yaml)?.name, "Sanshain");
    Ok(())
}

#[test]
fn a_line_without_direction_is_written_as_none() -> TestResult {
    let mut p = Project::new();
    let a = p.add_block(None, &BlockSpec::new("A", BlockKind::Component))?;
    p.connect_new(
        a,
        Side::Right,
        &BlockSpec::new("B", BlockKind::Component),
        Direction::Undirected,
        "",
    )?;
    let yaml = persist::to_yaml(&p)?;
    assert!(yaml.contains("direction: none"), "{yaml}");
    let back = persist::from_yaml(&yaml)?;
    let rel = back.relations.values().next().ok_or("relation")?;
    assert_eq!(rel.direction, Direction::Undirected);
    Ok(())
}

#[test]
fn bands_are_saved_without_a_cell() -> TestResult {
    let mut p = sample()?;
    p.add_block(
        None,
        &BlockSpec::new("Logging", BlockKind::Component).as_band(),
    )?;
    let yaml = persist::to_yaml(&p)?;
    let band = yaml
        .split("- id: ")
        .find(|b| b.contains("name: Logging"))
        .ok_or("band entry")?;
    assert!(
        band.contains("band: true") && !band.contains("cell:"),
        "{band}"
    );
    let back = persist::from_yaml(&yaml)?;
    assert_eq!(back.blocks, p.blocks);
    Ok(())
}

#[test]
fn a_band_with_lines_is_rejected() -> TestResult {
    let p = sample()?;
    let yaml =
        persist::to_yaml(&p)?.replacen("  kind: person\n", "  kind: component\n  band: true\n", 1);
    assert!(matches!(
        persist::from_yaml(&yaml),
        Err(PersistError::Invalid(_))
    ));
    Ok(())
}

#[test]
fn a_fanned_out_relation_round_trips_and_bad_trees_are_rejected() -> TestResult {
    let mut p = sample()?;
    let rel = *p.relations.keys().next().ok_or("relation")?;
    let shop = p.relation(rel)?.b.anchors[0].block;
    let extra = p.add_block(Some(shop), &BlockSpec::new("Extra", BlockKind::Component))?;
    p.attach(rel, End::B, shop, extra, Side::Left)?;
    let yaml = persist::to_yaml(&p)?;
    assert_eq!(persist::from_yaml(&yaml)?.relations, p.relations);
    // The same box twice in one end.
    let mut bad = p.clone();
    if let Some(r) = bad.relations.get_mut(&rel) {
        let again = r.b.anchors[1];
        r.b.anchors.push(again);
    }
    assert!(matches!(
        persist::from_yaml(&persist::to_yaml(&bad)?),
        Err(PersistError::Invalid(_))
    ));
    Ok(())
}

#[test]
fn the_export_choice_is_saved_with_the_project() -> TestResult {
    use whiteboxed::doc::DocFormat;
    use whiteboxed::model::ExportChoice;
    let mut p = sample()?;
    assert!(!persist::to_yaml(&p)?.contains("export:"));
    p.export = Some(ExportChoice {
        all: false,
        svg: true,
        png: false,
        png_scale: 3,
        text: Some(DocFormat::Markdown),
        html: true,
        folder: "../docs/arc42".into(),
    });
    let yaml = persist::to_yaml(&p)?;
    assert!(yaml.contains("export:\n  all: false\n"), "{yaml}");
    assert!(
        yaml.contains("  text: markdown\n  html: true\n  folder: ../docs/arc42\n"),
        "{yaml}"
    );
    assert_eq!(persist::from_yaml(&yaml)?.export, p.export);
    Ok(())
}

#[test]
fn score_limits_are_saved_only_when_changed() -> TestResult {
    use whiteboxed::model::ScoreLimits;
    let mut p = sample()?;
    assert!(!persist::to_yaml(&p)?.contains("score_limits"));
    p.set_score_limits(ScoreLimits {
        boxes: (12, 15),
        ..ScoreLimits::default()
    });
    let yaml = persist::to_yaml(&p)?;
    assert!(yaml.contains("score_limits:"), "{yaml}");
    assert_eq!(persist::from_yaml(&yaml)?.score_limits, p.score_limits);
    Ok(())
}
