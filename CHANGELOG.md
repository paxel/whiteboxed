# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.6.3]

### Added

- Every release file carries signed build provenance, which `gh attestation verify` checks as `SECURITY.md` describes, and every release a CycloneDX SBOM of its source and the crate file ([wbx-1002](https://github.com/paxel/whiteboxed/issues/14)).

### Security

- Updated rmcp to 3.11.0 for its unbounded tool schema cache in the Streamable HTTP server transport (RUSTSEC-2026-0461) and two client-side advisories (RUSTSEC-2026-0457, RUSTSEC-2026-0462) ([wbx-1000](https://github.com/paxel/whiteboxed/issues/12)).
- Updated url to 2.5.9 for a stack overflow in `Url::origin()` (RUSTSEC-2026-0592) and rustix to 1.1.6 for an unsound xattr call (RUSTSEC-2026-0486) ([wbx-1000](https://github.com/paxel/whiteboxed/issues/12)).

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
