//! The Sanshain model, a real project of about forty relations over five diagrams,
//! as the yardstick for the layout: every diagram's readability measures must stay
//! at least as good as the numbers recorded here.

use whiteboxed::geom::{Pos, Rect};
use whiteboxed::layout::{self, LANE, Layout};
use whiteboxed::model::{BlockId, Project};
use whiteboxed::{scene, view};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// What a drawing is judged by, worst first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Measures {
    crossings: usize,
    overlaps: usize,
    holes: usize,
    /// Labels on top of each other or of a hole's label, over a box, or with another
    /// line through them.
    label_collisions: usize,
    /// Drawn segments through the whitebox title or a cross-cutting band.
    through_title_or_band: usize,
    /// Steps shorter than half a lane between two runs along one axis.
    short_steps: usize,
    /// Partner names at the frame with another line through them.
    crossed_names: usize,
}

fn sanshain() -> Result<Project, Box<dyn std::error::Error>> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sanshain.yaml");
    Ok(whiteboxed::persist::load(std::path::Path::new(path))?)
}

fn diagram(p: &Project, path: &str) -> Option<BlockId> {
    if path.is_empty() {
        return None;
    }
    let mut parent = None;
    for name in path.split('/') {
        parent = p
            .blocks
            .iter()
            .find(|(_, b)| b.parent == parent && b.name == name)
            .map(|(id, _)| *id);
        parent?;
    }
    parent
}

fn measure(p: &Project, l: &Layout) -> Measures {
    let crossings = layout::crossings(l).len();
    let overlaps = layout::overlaps(l).len();
    let holes = l.lines.iter().filter(|g| g.mousehole.is_some()).count();
    let labels = scene::label_rects(l);
    let rel = |li: usize| l.lines[li].relation;
    let hole_labels: Vec<Rect> = l.lines.iter().flat_map(scene::hole_label_rects).collect();
    let mut label_collisions = 0;
    for (i, (a, ra)) in labels.iter().enumerate() {
        label_collisions += hole_labels.iter().filter(|h| h.overlaps(ra)).count();
        for (b, rb) in &labels[i + 1..] {
            if rel(*a) != rel(*b) && ra.overlaps(rb) {
                label_collisions += 1;
            }
        }
        if l.blocks.iter().any(|b| b.rect.overlaps(ra)) {
            label_collisions += 1;
        }
        let crossed = l.lines.iter().enumerate().any(|(lj, g)| {
            rel(lj) != rel(*a)
                && g.drawn().into_iter().any(|piece| {
                    piece
                        .windows(2)
                        .any(|w| layout::segment_meets(w[0], w[1], ra))
                })
        });
        if crossed {
            label_collisions += 1;
        }
    }
    let mut keep_off: Vec<Rect> = l.blocks.iter().filter(|b| b.band).map(|b| b.rect).collect();
    if let (Some(frame), Some(title)) = (l.frame, &l.title) {
        let title = format!("{title} (whitebox)");
        let w = whiteboxed::text::width(&title, layout::NAME_SIZE);
        keep_off.push(Rect::from_min_size(
            Pos::new(
                frame.min.x + 16.0,
                frame.min.y + 20.0 - layout::NAME_SIZE * 0.75,
            ),
            w,
            layout::NAME_SIZE,
        ));
    }
    let through_title_or_band = l
        .lines
        .iter()
        .flat_map(|g| g.drawn())
        .flat_map(|piece| piece.windows(2).map(|w| (w[0], w[1])))
        .filter(|(a, b)| {
            keep_off
                .iter()
                .any(|r| layout::segment_meets(*a, *b, &r.expand(-0.5)))
        })
        .count();
    let short_steps = l
        .lines
        .iter()
        .filter(|g| g.tip.is_none())
        .flat_map(|g| g.points.windows(3).collect::<Vec<_>>())
        .filter(|w| w[0].dist(w[1]) > 0.0 && w[1].dist(w[2]) < LANE / 2.0 && w[1].dist(w[2]) > 0.01)
        .count();
    let crossed_names = scene::frame_label_rects(l)
        .into_iter()
        .filter(|(li, r)| {
            l.lines.iter().enumerate().any(|(lj, g)| {
                lj != *li
                    && g.drawn().into_iter().any(|piece| {
                        piece
                            .windows(2)
                            .any(|w| layout::segment_meets(w[0], w[1], r))
                    })
            })
        })
        .count();
    let _ = p;
    Measures {
        crossings,
        overlaps,
        holes,
        label_collisions,
        through_title_or_band,
        short_steps,
        crossed_names,
    }
}

/// Recorded with the layout of 23732d7; a change may improve on it, never fall
/// behind.
const DIAGRAMS: [(&str, Measures); 5] = [
    (
        "",
        Measures {
            crossings: 0,
            overlaps: 0,
            holes: 0,
            label_collisions: 0,
            through_title_or_band: 0,
            short_steps: 0,
            crossed_names: 0,
        },
    ),
    (
        "Sanshain Service",
        Measures {
            crossings: 1,
            overlaps: 0,
            holes: 1,
            label_collisions: 0,
            through_title_or_band: 0,
            short_steps: 0,
            crossed_names: 0,
        },
    ),
    (
        "Sanshain Service/HTTP API",
        Measures {
            crossings: 4,
            overlaps: 0,
            holes: 1,
            label_collisions: 0,
            through_title_or_band: 0,
            short_steps: 0,
            crossed_names: 0,
        },
    ),
    (
        "Sanshain Service/Application Services",
        Measures {
            crossings: 6,
            overlaps: 0,
            holes: 0,
            label_collisions: 0,
            through_title_or_band: 0,
            short_steps: 1,
            crossed_names: 0,
        },
    ),
    (
        "Sanshain Service/Persistence",
        Measures {
            crossings: 0,
            overlaps: 0,
            holes: 0,
            label_collisions: 0,
            through_title_or_band: 0,
            short_steps: 0,
            crossed_names: 0,
        },
    ),
];

#[test]
fn the_sanshain_diagrams_stay_as_readable_as_recorded() -> TestResult {
    let p = sanshain()?;
    let mut worse = Vec::new();
    for (path, recorded) in DIAGRAMS {
        let d = diagram(&p, path);
        assert!(path.is_empty() || d.is_some(), "no diagram {path}");
        let l = layout::layout(&p, &view::diagram_view(&p, d));
        let now = measure(&p, &l);
        println!("{path:?}: {now:?}");
        let fields = [
            ("crossings", now.crossings, recorded.crossings),
            ("overlaps", now.overlaps, recorded.overlaps),
            ("holes", now.holes, recorded.holes),
            (
                "label_collisions",
                now.label_collisions,
                recorded.label_collisions,
            ),
            (
                "through_title_or_band",
                now.through_title_or_band,
                recorded.through_title_or_band,
            ),
            ("short_steps", now.short_steps, recorded.short_steps),
            ("crossed_names", now.crossed_names, recorded.crossed_names),
        ];
        for (name, got, allowed) in fields {
            if got > allowed {
                worse.push(format!("{path}: {name} {got} > {allowed}"));
            }
        }
    }
    assert!(worse.is_empty(), "{worse:#?}");
    Ok(())
}
