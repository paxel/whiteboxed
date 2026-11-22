## Agent skills

### Issue tracker

Issues are tracked in GitHub Issues on paxel/whiteboxed (via `gh`). See `docs/agents/issue-tracker.md`.

### Triage labels

Default vocabulary: needs-triage, needs-info, ready-for-agent, ready-for-human, wontfix. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `CONTEXT.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

### Ticket IDs

Prefix `wbx`, see `docs/agents/ticket-ids.md`.

## Gates

whiteboxed follows the [paxel/gates](https://github.com/paxel/gates) standard; its choices are
in `docs/adr/0001-move-to-gates.md`.

* Coverage ratchet: the Rust gate in `ci.yml` fails under the floor in `coverage-floors.toml`. When coverage rises, raise the floor and the badge in the same pull request, on a machine with a Vulkan device or lavapipe, with gates checked out beside the repository: `cargo tarpaulin --engine llvm --locked --all-features --skip-clean --out Lcov --output-dir coverage && mv coverage/lcov.info coverage/rust.lcov`, then `python3 ../gates/scripts/coverage_ratchet.py --floors coverage-floors.toml`. Never lower it.
* Accepted findings carry a reason and a date: advisories in `osv-scanner.toml`, code-scanning and SonarQube Cloud findings in `code-scanning-ignores.toml`. Nothing is dismissed in GitHub.
