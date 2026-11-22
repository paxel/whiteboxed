# 1. whiteboxed moves to the gates standard

Date: 2026-11-22

## Status

Accepted.

## Context

whiteboxed had its own CI, release gate and Security workflow (cargo audit and cargo deny,
daily). [paxel/gates](https://github.com/paxel/gates) is the one standard every paxel
repository is held to, as shared workflows a repository calls; its weekly audit found 25 of
its checks missing here (#11). At the same time the daily Security run had been red for days
on advisories for rmcp, url and rustix.

## Decision

whiteboxed meets the standard of gates v1.4.1. Where the standard decides, it is followed
without a note here: the OSV gate instead of cargo audit and cargo deny, one coverage ratchet,
the shared CodeQL, Scorecard, fuzz, release gate and provenance workflows, the files and the
badge row. What follows is where whiteboxed adds to it, or had a choice.

**Test lanes.** The shared Rust gate runs format, the lint set, clippy, the tests and the
coverage ratchet on Linux x86_64; its other runners only run clippy. whiteboxed ships for
Windows and macOS, so `ci.yml` keeps two lanes of its own that build and run every test there
([wbx-1001](https://github.com/paxel/whiteboxed/issues/13)).

**Render tests.** The Rust gate runs with all features, so the render tests
(`tests/render.rs`) run there and count toward coverage, on software Vulkan (lavapipe) that
`apt-packages` installs. The Windows and macOS runners have no Vulkan device and run without
them.

**Lint set.** The four lints that `lib.rs` and `main.rs` denied outside `cfg(test)`
(`unreachable`, indexing, slicing, overflowing arithmetic) moved into `[lints]` with the rest
of the set. Tests are exempt the way the shared gate lints them: a second clippy run with
those lints allowed on the command line. `unsafe_code` stays `forbid`, as whiteboxed has none.

**Release.** The shared release gate checks alerts, findings, vulnerabilities, licences and
the changelog section, but runs no tests. A tag can sit on any commit, and a release feeds
crates.io, the Homebrew tap and the Scoop bucket, none of which can be taken back cleanly, so
the release keeps a job of its own that checks the tag against the crate version and runs
format, clippy and every test, render tests included, on the tagged commit
([wbx-1002](https://github.com/paxel/whiteboxed/issues/14)).

**The crate.** The standard counts a registry package as a release file (its Q8). A job packs
the `.crate` file, which is attested and attached to the GitHub release; `cargo publish` runs
last, after the release, as before.

**Fuzzing.** Two targets, for the two parsers that read files from someone else:
`from_yaml` (project files) and `look_sanitize` (the SVG of a custom look), a quarter of an
hour each, weekly ([wbx-1004](https://github.com/paxel/whiteboxed/issues/16)).

**Unmaintained crates.** Five unmaintained notices are accepted in `osv-scanner.toml` until
2027-02-22, three months, because no update removes them
([wbx-1000](https://github.com/paxel/whiteboxed/issues/12)):

- chrono (RUSTSEC-2026-0543), through rmcp and schemars; rmcp 3.11.0 still needs it.
- ttf-parser (RUSTSEC-2026-0192), for the embedded font and winit's Wayland window decorations (audit finding S11).
- zvariant, zbus_names and zvariant_derive (RUSTSEC-2026-0370, -0371, -0372), through
  eframe's accesskit feature, which makes the UI visible to screen readers. No release of
  accesskit_unix on zbus 6 exists; dropping the feature was rejected, as in pixpixpix.

**Scorecard.** "Vulnerabilities" lists the notices above and is accepted with them.
"Maintained" and "SAST" fire for a repository younger than 90 days whose first commits went
straight to `summer`; both are accepted until 2027-01-05, when they end by themselves.

**Required checks.** The ruleset on `summer` requires the shared Rust gate, the OSV gate,
SonarQube Cloud, the Windows and macOS lanes and CodeQL, with no approval, as there is a
single maintainer.

## Consequences

- cargo deny's checks for yanked crates and for crates from unknown registries or git sources
  have no counterpart in the OSV gate and are lost.
- The Security run is weekly and strict instead of daily: unmaintained notices block it too.
- SonarQube Cloud analyses from CI, with coverage, instead of Automatic Analysis.
- A release carries a CycloneDX SBOM of its source, the `.crate` file, and its Sigstore
  bundle named `whiteboxed-<tag>-provenance.sigstore.json`.
