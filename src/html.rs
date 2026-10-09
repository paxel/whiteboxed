//! The HTML export: one self-contained file with every diagram, its texts and tables.
//! Diagrams are inline SVG with an invisible layer on top that makes boxes, lines and
//! frame ends clickable; a tree on the left and breadcrumbs lead through the levels.

use std::fmt::Write;

use base64::Engine;

use crate::doc::{self, RowKey};
use crate::geom::Rect;
use crate::layout::{self, Layout};
use crate::model::{BlockId, DiagramId, Project};
use crate::view;
use crate::{export, scene};

/// The whole document. `title` names the system.
pub fn html_doc(project: &Project, title: &str) -> String {
    let diagrams = doc::diagrams(project);
    let mut out = String::new();
    out.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    let _ = writeln!(out, "<title>{}</title>", esc(title));
    let font = base64::engine::general_purpose::STANDARD.encode(epaint_default_fonts::UBUNTU_LIGHT);
    let _ = writeln!(
        out,
        "<style>@font-face {{ font-family: \"Ubuntu\"; font-weight: 300; src: url(data:font/ttf;base64,{font}) format(\"truetype\"); }}</style>"
    );
    out.push_str(STYLE);
    out.push_str("</head>\n<body>\n");
    out.push_str("<div class=\"tools\"><button id=\"theme\" type=\"button\">Dark</button></div>\n");
    out.push_str("<div class=\"page\">\n<nav class=\"rail\" aria-label=\"Diagrams\">\n");
    let _ = writeln!(
        out,
        "<div class=\"doc\">{}<span>Building block view · arc42</span></div>",
        esc(title)
    );
    out.push_str("<div class=\"label\">Diagrams</div>\n");
    tree(&mut out, project, &diagrams, None);
    out.push_str("</nav>\n<main>\n");
    for d in &diagrams {
        section(&mut out, project, *d);
    }
    out.push_str("</main>\n</div>\n");
    out.push_str(SCRIPT);
    out.push_str("</body>\n</html>\n");
    out
}

/// The id of a diagram's section.
pub fn section_id(diagram: DiagramId) -> String {
    match diagram {
        None => "context".into(),
        Some(id) => format!("wb-{}", id.0),
    }
}

fn name(project: &Project, id: BlockId) -> String {
    project
        .blocks
        .get(&id)
        .map_or_else(|| "?".to_owned(), |b| b.name.clone())
}

/// The diagrams as a nested list: every whitebox under the diagram its box lies in.
fn tree(out: &mut String, project: &Project, diagrams: &[DiagramId], parent: Option<DiagramId>) {
    let children: Vec<DiagramId> = diagrams
        .iter()
        .copied()
        .filter(|d| match (parent, d) {
            (None, None) => true,
            (Some(p), Some(id)) => project.blocks.get(id).is_some_and(|b| b.parent == p),
            _ => false,
        })
        .collect();
    if children.is_empty() {
        return;
    }
    out.push_str(if parent.is_none() {
        "<ul class=\"tree\">"
    } else {
        "<ul>"
    });
    for d in children {
        let label = match d {
            None => "Context".to_owned(),
            Some(id) => name(project, id),
        };
        let _ = write!(
            out,
            "<li><a href=\"#{}\">{}</a>",
            section_id(d),
            esc(&label)
        );
        tree(out, project, diagrams, Some(d));
        out.push_str("</li>");
    }
    out.push_str("</ul>\n");
}

fn section(out: &mut String, project: &Project, diagram: DiagramId) {
    let text = doc::diagram_text(project, diagram);
    let parent = diagram
        .and_then(|id| project.blocks.get(&id))
        .map(|b| section_id(b.parent))
        .unwrap_or_default();
    let _ = writeln!(
        out,
        "<section class=\"diagram\" id=\"{}\" data-parent=\"{parent}\">",
        section_id(diagram)
    );
    let mut crumbs = vec![("context".to_owned(), "Context".to_owned())];
    for id in project.path(diagram) {
        crumbs.push((section_id(Some(id)), name(project, id)));
    }
    let crumb: Vec<String> = crumbs
        .iter()
        .map(|(id, n)| format!("<a href=\"#{id}\">{}</a>", esc(n)))
        .collect();
    let _ = writeln!(
        out,
        "<p class=\"crumb\">{}</p>",
        crumb.join(" <span>›</span> ")
    );
    let level = project.level(diagram);
    let level = if level == 0 {
        "context view".to_owned()
    } else {
        format!("level {level}")
    };
    let _ = writeln!(
        out,
        "<h2>{} <span class=\"level\">{level}</span></h2>",
        esc(&text.title)
    );
    let l = layout::layout(project, &view::diagram_view(project, diagram));
    let _ = writeln!(
        out,
        "<figure class=\"paper\"><div class=\"svgwrap\">{}</div></figure>",
        clickable_svg(project, &l)
    );
    if !text.motivation.is_empty() {
        if text.motivation_heading {
            out.push_str("<h3>Motivation</h3>\n");
        }
        let _ = writeln!(out, "<p class=\"prose\">{}</p>", lines(&text.motivation));
    }
    for table in &text.tables {
        let _ = writeln!(out, "<h3>{}</h3>", esc(table.title));
        out.push_str("<div class=\"tablewrap\"><table><thead><tr>");
        for h in table.header {
            let _ = write!(out, "<th scope=\"col\">{}</th>", esc(h));
        }
        out.push_str("</tr></thead><tbody>\n");
        for row in &table.rows {
            let key = match row.key {
                RowKey::Box(id) => format!("data-box=\"{}\"", id.0),
                RowKey::Relation(id) => format!("data-rel=\"{}\"", id.0),
            };
            let _ = write!(out, "<tr {key}>");
            for (k, cell) in row.cells.iter().enumerate() {
                let content = if cell.trim().is_empty() {
                    "<span class=\"none\">–</span>".to_owned()
                } else {
                    lines(cell)
                };
                // A box with a whitebox of its own links to it.
                let link = match (k, row.key) {
                    (0, RowKey::Box(id)) if project.has_content(id) && Some(id) != diagram => {
                        format!(
                            " <a class=\"open\" href=\"#{}\">whitebox ↓</a>",
                            section_id(Some(id))
                        )
                    }
                    _ => String::new(),
                };
                let _ = write!(out, "<td>{content}{link}</td>");
            }
            out.push_str("</tr>\n");
        }
        out.push_str("</tbody></table></div>\n");
    }
    out.push_str("</section>\n");
}

/// The diagram's SVG with a transparent layer of click targets on top.
fn clickable_svg(project: &Project, l: &Layout) -> String {
    let svg = export::to_svg(&scene::scene(l));
    let mut hits = String::from("<g class=\"hits\">\n");
    let rect = |r: &Rect| {
        format!(
            "x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\"",
            r.min.x,
            r.min.y,
            r.width(),
            r.height()
        )
    };
    for b in &l.blocks {
        let open = if project.has_content(b.id) {
            format!(" data-open=\"{}\"", section_id(Some(b.id)))
        } else {
            String::new()
        };
        let _ = writeln!(
            hits,
            "<rect class=\"hit\" fill=\"transparent\" data-box=\"{}\"{open} {}><title>{}</title></rect>",
            b.id.0,
            rect(&b.rect),
            esc(&b.name)
        );
    }
    // Boxes at both ends, for diagrams whose tables list partners instead of relations.
    let ends = |li: usize| -> String {
        let Some(rel) = l
            .lines
            .get(li)
            .and_then(|g| project.relations.get(&g.relation))
        else {
            return String::new();
        };
        [&rel.a, &rel.b]
            .iter()
            .filter_map(|e| e.anchors.first())
            .map(|a| a.block.0.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    };
    for (li, line) in l.lines.iter().enumerate() {
        let points: Vec<String> = line
            .points
            .iter()
            .map(|p| format!("{:.1},{:.1}", p.x, p.y))
            .collect();
        let _ = writeln!(
            hits,
            "<polyline class=\"hit\" fill=\"none\" stroke=\"transparent\" stroke-width=\"12\" data-rel=\"{}\" data-ends=\"{}\" points=\"{}\"><title>{}</title></polyline>",
            line.relation.0,
            ends(li),
            points.join(" "),
            esc(&line.full_text)
        );
    }
    for (li, r) in scene::label_rects(l) {
        let Some(line) = l.lines.get(li) else {
            continue;
        };
        let _ = writeln!(
            hits,
            "<rect class=\"hit\" fill=\"transparent\" data-rel=\"{}\" data-ends=\"{}\" {}><title>{}</title></rect>",
            line.relation.0,
            ends(li),
            rect(&r),
            esc(&line.full_text)
        );
    }
    // A frame end leads to the partner outside, in the diagram that shows it.
    if let Some(owner) = l.diagram {
        for (li, r) in scene::frame_label_rects(l) {
            let Some(line) = l.lines.get(li) else {
                continue;
            };
            let Some(rel) = project.relations.get(&line.relation) else {
                continue;
            };
            let partner = [&rel.a, &rel.b]
                .into_iter()
                .find(|e| e.position_of(owner).is_none())
                .and_then(|e| e.anchors.first())
                .map(|a| a.block);
            let Some(partner) = partner else { continue };
            let Some(home) = project.blocks.get(&partner).map(|b| section_id(b.parent)) else {
                continue;
            };
            let port = line
                .frame_port
                .as_ref()
                .map(|(p, _, _)| Rect::from_center(*p, 16.0, 16.0));
            for target in std::iter::once(r).chain(port) {
                let _ = writeln!(
                    hits,
                    "<rect class=\"hit\" fill=\"transparent\" data-partner=\"{}\" data-goto=\"{home}\" {}><title>{}</title></rect>",
                    partner.0,
                    rect(&target),
                    esc(&name(project, partner))
                );
            }
        }
    }
    hits.push_str("</g>\n");
    match svg.rfind("</svg>") {
        Some(end) => format!("{}{hits}{}", &svg[..end], &svg[end..]),
        None => svg,
    }
}

/// Text for HTML, with line breaks kept.
fn lines(text: &str) -> String {
    text.trim()
        .lines()
        .map(esc)
        .collect::<Vec<_>>()
        .join("<br>")
}

fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

const STYLE: &str = r#"<style>
/* A fixed rail with the diagram tree, a reading column of arc42 sections; diagrams sit
   on white paper in both themes, printing is always light. */
:root {
  --bg: #f6f7f9; --surface: #ffffff; --fg: #1f2630; --muted: #5e6875; --rule: #d9dee5;
  --accent: #1d6fb8; --lit: #fff1c2; --lit-edge: #e0b432;
  --body: system-ui, -apple-system, "Segoe UI", "Ubuntu", sans-serif;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --bg: #14181d; --surface: #1c2229; --fg: #e3e8ee; --muted: #9aa5b2; --rule: #2e3742;
    --accent: #6fb0ec; --lit: #4a3d12; --lit-edge: #d6a72a; color-scheme: dark;
  }
}
:root[data-theme="dark"] {
  --bg: #14181d; --surface: #1c2229; --fg: #e3e8ee; --muted: #9aa5b2; --rule: #2e3742;
  --accent: #6fb0ec; --lit: #4a3d12; --lit-edge: #d6a72a; color-scheme: dark;
}
@media print {
  :root, :root[data-theme="dark"], :root:not([data-theme="light"]) {
    --bg: #ffffff; --surface: #ffffff; --fg: #000000; --muted: #444444; --rule: #bbbbbb;
    --accent: #000000; --lit: #ffffff; --lit-edge: #bbbbbb; color-scheme: light;
  }
  .rail, .tools { display: none !important; }
  .page { display: block; }
  section.diagram { break-before: page; }
}
* { box-sizing: border-box; }
html, body { margin: 0; }
body { background: var(--bg); color: var(--fg); font-family: var(--body); line-height: 1.55; }
a { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }
a:focus-visible, button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.tools { position: fixed; top: 12px; right: 16px; z-index: 10; }
.tools button { font: inherit; font-size: 0.85rem; color: var(--fg); background: var(--surface);
  border: 1px solid var(--rule); border-radius: 6px; padding: 5px 10px; cursor: pointer;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.12); }
.page { display: grid; grid-template-columns: 250px minmax(0, 1fr); min-height: 100vh; }
.rail { position: sticky; top: 0; align-self: start; height: 100vh; overflow: auto;
  border-right: 1px solid var(--rule); padding: 24px 20px; display: grid; align-content: start; gap: 18px; }
.rail .doc { font-weight: 600; font-size: 1.05rem; line-height: 1.3; }
.rail .doc span { display: block; font-weight: 400; font-size: 0.8rem; color: var(--muted); }
.label { font-size: 0.72rem; letter-spacing: 0.08em; text-transform: uppercase; color: var(--muted); }
.tree, .tree ul { list-style: none; margin: 0; padding: 0; }
.tree ul { padding-left: 14px; border-left: 1px solid var(--rule); margin-left: 6px; }
.tree a { display: block; padding: 3px 8px; border-radius: 5px; color: var(--fg); }
.tree a.here { background: var(--accent); color: var(--surface); }
main { padding: 56px 40px 80px; display: grid; gap: 64px; min-width: 0; }
section.diagram { display: grid; gap: 14px; min-width: 0; scroll-margin-top: 16px; }
.crumb { margin: 0; font-size: 0.85rem; color: var(--muted); }
.crumb span { margin: 0 4px; }
h2 { margin: 0; font-size: 1.7rem; font-weight: 600; text-wrap: balance; }
h2 .level { font-size: 0.8rem; font-weight: 400; color: var(--muted); margin-left: 8px; }
h3 { margin: 18px 0 0; font-size: 1.05rem; font-weight: 600; }
.prose { margin: 0; max-width: 72ch; }
.paper { margin: 0; background: #ffffff; border: 1px solid var(--rule); border-radius: 8px; padding: 8px; }
.svgwrap { overflow-x: auto; }
.svgwrap svg { display: block; width: 100%; height: auto; min-width: 640px; }
.hit { cursor: pointer; pointer-events: all; }
polyline.hit { pointer-events: stroke; }
.hit.hot { stroke: #1d6fb8; stroke-opacity: 0.45; stroke-width: 6; }
polyline.hit.hot { stroke-width: 8; }
.tablewrap { overflow-x: auto; }
table { border-collapse: collapse; width: 100%; font-size: 0.9rem; background: var(--surface); border: 1px solid var(--rule); }
th { text-align: left; font-weight: 600; padding: 8px 10px; border-bottom: 1px solid var(--rule); white-space: nowrap; }
td { padding: 8px 10px; border-top: 1px solid var(--rule); vertical-align: top; }
td:first-child { white-space: nowrap; }
tr.lit td { background: var(--lit); }
tr.lit td:first-child { box-shadow: inset 3px 0 0 var(--lit-edge); }
tr[data-rel] { cursor: pointer; }
.none { color: var(--muted); }
.open { font-size: 0.75rem; margin-left: 6px; }
@media (max-width: 760px) {
  .page { grid-template-columns: minmax(0, 1fr); }
  .rail { position: static; height: auto; border-right: 0; border-bottom: 1px solid var(--rule); padding: 20px 16px; }
  main { padding: 56px 16px 80px; }
}
@media (prefers-reduced-motion: no-preference) { tr td { transition: background 0.3s; } }
</style>
"#;

const SCRIPT: &str = r#"<script>
(() => {
  const root = document.documentElement;
  const btn = document.getElementById('theme');
  const dark = () => root.dataset.theme ? root.dataset.theme === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches;
  const label = () => { btn.textContent = dark() ? 'Light' : 'Dark'; };
  btn.addEventListener('click', () => { root.dataset.theme = dark() ? 'light' : 'dark'; label(); });
  label();
  const clear = () => document.querySelectorAll('.lit, .hot').forEach(e => e.classList.remove('lit', 'hot'));
  const light = rows => {
    clear();
    rows.forEach(r => r.classList.add('lit'));
    if (rows[0]) rows[0].scrollIntoView({ behavior: 'smooth', block: 'center' });
  };
  const go = id => {
    const s = document.getElementById(id);
    if (!s) return null;
    clear();
    s.scrollIntoView({ behavior: 'smooth', block: 'start' });
    history.replaceState(null, '', '#' + id);
    return s;
  };
  const rows = (sec, attr, value) => [...sec.querySelectorAll('tbody tr[' + attr + '="' + value + '"]')];
  document.querySelectorAll('section.diagram').forEach(sec => {
    sec.querySelectorAll('.hit').forEach(h => h.addEventListener('click', () => {
      const d = h.dataset;
      if (d.open) { go(d.open); return; }
      if (d.box) { light(rows(sec, 'data-box', d.box)); return; }
      if (d.rel) {
        let found = rows(sec, 'data-rel', d.rel);
        if (!found.length) found = (d.ends || '').split(' ').flatMap(b => b ? rows(sec, 'data-box', b) : []);
        light(found);
        sec.querySelectorAll('.hit[data-rel="' + d.rel + '"]').forEach(x => x.classList.add('hot'));
        return;
      }
      if (d.partner) {
        const home = go(d.goto);
        if (home) setTimeout(() => light(rows(home, 'data-box', d.partner)), 350);
      }
    }));
    // A row of an interface table points back at its line in the diagram.
    sec.querySelectorAll('tbody tr[data-rel]').forEach(r => r.addEventListener('click', e => {
      if (e.target.closest('a')) return;
      clear();
      r.classList.add('lit');
      sec.querySelectorAll('.hit[data-rel="' + r.dataset.rel + '"]').forEach(x => x.classList.add('hot'));
      sec.querySelector('figure').scrollIntoView({ behavior: 'smooth', block: 'center' });
    }));
  });
  const links = [...document.querySelectorAll('.tree a')];
  const mark = id => links.forEach(a => a.classList.toggle('here', a.getAttribute('href') === '#' + id));
  const seen = new IntersectionObserver(es => es.forEach(e => { if (e.isIntersecting) mark(e.target.id); }), { rootMargin: '-20% 0px -70% 0px' });
  document.querySelectorAll('section.diagram').forEach(s => seen.observe(s));
  mark(location.hash.slice(1) || 'context');
})();
</script>
"#;
