use whiteboxed::editor::{Editor, Popup};
use whiteboxed::layout;
use whiteboxed::model::{BlockKind, BlockSpec, Direction, ModelError, Project, ProjectKind, Side};
use whiteboxed::persist;
use whiteboxed::view::diagram_view;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn shape(text: &str) -> BlockSpec {
    BlockSpec::new(text, BlockKind::Component)
}

#[test]
fn a_doodle_has_pages_with_shapes_free_of_arc42_rules() -> TestResult {
    let mut p = Project::new_doodle();
    assert_eq!(p.kind, ProjectKind::Doodle);
    let pages = p.pages();
    assert_eq!(pages.len(), 1);
    let page = pages[0];
    // The same text twice, over two lines, and a person on a page.
    let a = p.add_block(Some(page), &shape("Check token\nvalid?"))?;
    let b = p.add_block(Some(page), &shape("Check token\nvalid?"))?;
    p.add_block(Some(page), &BlockSpec::new("User", BlockKind::Person))?;
    assert_eq!(p.block(a)?.name, "Check token\nvalid?");
    p.connect_existing(
        a,
        Side::Right,
        b,
        Direction::Out,
        "",
        whiteboxed::model::Placement::Keep,
    )?;
    // No shapes at the top, none inside shapes, no open ends, no automatic name.
    assert_eq!(
        p.add_block(None, &shape("X")),
        Err(ModelError::PageOutOfPlace)
    );
    assert_eq!(
        p.add_block(Some(a), &shape("X")),
        Err(ModelError::PageOutOfPlace)
    );
    assert_eq!(
        p.add_stub(a, Side::Top, Direction::Out, ""),
        Err(ModelError::NoOpenEnds)
    );
    assert_eq!(p.auto_name(), None);
    // Pages: names stay unique, the last one stays.
    let second = p.add_page("Network")?;
    assert!(p.add_page("network").is_err());
    p.delete_block(second)?;
    assert_eq!(p.delete_block(page), Err(ModelError::LastPage));
    Ok(())
}

#[test]
fn arc42_projects_keep_their_rules() -> TestResult {
    let mut p = Project::new();
    assert_eq!(p.add_page("Page"), Err(ModelError::PageOutOfPlace));
    let shop = p.add_block(None, &shape("Shop"))?;
    assert_eq!(
        p.add_block(Some(shop), &BlockSpec::new("User", BlockKind::Person)),
        Err(ModelError::NeighbourBelowContext("person"))
    );
    assert_eq!(
        p.add_block(None, &shape("shop")),
        Err(ModelError::DuplicateName("shop".into()))
    );
    assert_eq!(
        p.add_block(None, &shape("two\nlines")),
        Err(ModelError::ControlCharacter)
    );
    Ok(())
}

#[test]
fn a_doodle_is_saved_with_its_kind_and_checked_on_load() -> TestResult {
    let mut p = Project::new_doodle();
    let page = p.pages()[0];
    p.add_block(Some(page), &shape("A"))?;
    let yaml = persist::to_yaml(&p)?;
    assert!(yaml.starts_with("format: 1\nkind: doodle\n"), "{yaml}");
    let back = persist::from_yaml(&yaml)?;
    assert_eq!(back, p);
    // An arc42 file does not mention its kind.
    assert!(!persist::to_yaml(&Project::new())?.contains("kind:"));
    // A page in an arc42 file is refused.
    let broken = yaml.replace("kind: doodle\n", "");
    assert!(persist::from_yaml(&broken).is_err());
    Ok(())
}

#[test]
fn a_doodle_page_has_no_frame_and_no_type_captions() -> TestResult {
    let mut p = Project::new_doodle();
    let page = p.pages()[0];
    p.add_block(Some(page), &shape("A"))?;
    let l = layout::layout(&p, &diagram_view(&p, Some(page)));
    assert!(l.frame.is_none() && l.title.is_none());
    assert!(!l.captions);
    Ok(())
}

#[test]
fn the_editor_always_shows_a_page() -> TestResult {
    let mut e = Editor::new_doodle(None);
    let first = e.project.pages()[0];
    assert_eq!(e.diagram, Some(first));
    e.go_up();
    assert_eq!(e.diagram, Some(first));
    e.add_page();
    let second = e.diagram.ok_or("no page")?;
    assert_ne!(second, first);
    assert_eq!(e.project.block(second)?.name, "Page 2");
    e.request_delete(second);
    if e.popup.is_some() {
        e.confirm();
    }
    assert_eq!(e.diagram, Some(first));
    assert!(e.scores().is_empty());
    // Shapes do not open into anything.
    let a = e.project.add_block(Some(first), &shape("A"))?;
    e.open_diagram(Some(a));
    assert_eq!(e.diagram, Some(first));
    assert!(!matches!(e.popup, Some(Popup::Restore(_))));
    Ok(())
}
