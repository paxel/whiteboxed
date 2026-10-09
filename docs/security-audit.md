# Security audit — 2026-10-08

whiteboxed is a desktop app. Since it offers AI access (an MCP endpoint, see
[Guide › AI access](guide.md#ai-access-mcp)), a program on the same machine — or an
AI agent running in a container that can reach the host — may drive it. This audit
asks one question first: **can whoever holds the AI token do more than edit the open
architecture model?** It also covers the files the app reads and writes, its
dependencies and its build pipeline.

Version audited: `0.1.0` (unreleased), branch `summer`; findings fixed up to commit
`5c1a625`. Updated for `0.2.0` (S15, S16): the dependency tree is unchanged since
`0.1.0`, so the tool results below still apply; the Security workflow re-checks them
on every push.

## Summary

| #   | Finding                                                        | Severity | Status     |
|-----|----------------------------------------------------------------|----------|------------|
| S1  | `export_docs` wrote into any folder the AI named               | High     | Fixed      |
| S2  | A box path with special characters crashed the app             | High     | Fixed      |
| S3  | Typed text could inject AsciiDoc includes or HTML into exports | Medium   | Fixed      |
| S4  | Browser pages were not refused by Origin                       | Medium   | Fixed      |
| S5  | An AI could flood the model until the window froze             | Medium   | Fixed      |
| S6  | A bug in a tool call would close the app                       | Medium   | Fixed      |
| S7  | Control characters broke SVG export and rendering              | Low      | Fixed      |
| S8  | Menu entry temp file could be written through a symlink        | Low      | Fixed      |
| S9  | Recovery files were readable by other local users              | Low      | Fixed      |
| S10 | Project files are parsed by a C-translated YAML parser         | Low      | Accepted   |
| S11 | `ttf-parser` is unmaintained                                   | Low      | Accepted   |
| S12 | The token can leak through the client side                     | Low      | Documented |
| S13 | CI actions were pinned by tag, not by commit                   | Low      | Fixed      |
| S14 | Many open MCP sessions use memory                              | Info     | Accepted   |
| S15 | AI access can listen beyond this computer (0.2.0)              | Medium   | Mitigated  |
| S16 | `batch` could hide a forbidden call among allowed ones (0.2.0) | Low      | Fixed      |

No known vulnerability (CVE / RustSec advisory) affects any of the 508 crates in
`Cargo.lock`. whiteboxed itself contains no `unsafe` code and now forbids it.

## Threat model

| Who                                    | Can reach                                                        | Needs                             |
|----------------------------------------|------------------------------------------------------------------|-----------------------------------|
| The user                               | Everything                                                       | —                                 |
| An AI client the user registered       | The 18 MCP tools                                                 | The token                         |
| Another program or user on the machine | `127.0.0.1:<port>` while AI access is on                         | The token                         |
| An AI agent in a container             | The Docker bridge, only if the user chose "Docker containers"    | The token                         |
| Another machine on the network         | Only an address the user typed under "Other address"             | The token                         |
| A web page in the user's browser       | `127.0.0.1:<port>` (browsers allow it)                           | Refused by Origin and Host checks |
| A project file from someone else       | The YAML parser and the layout                                   | The user opening it               |

What the token grants, checked tool by tool in `src/api.rs`: reading and changing
the open model, rendering it to PNG, and `export_docs` into folders the user allowed
(S1). **No tool reads files, lists directories, runs commands, opens URLs, saves or
opens projects.**

## Findings

### S1 `export_docs` wrote into any folder the AI named — High, fixed

`export_docs` accepted any absolute path, created missing directories and wrote
`context*.svg/.png/.adoc/.md` and `index.adoc`/`index.md` there, overwriting files of
those names and following symlinks.

On a normal desktop with Claude Code this added nothing: Claude Code can already write
files with the user's rights. It mattered when the AI is **confined**: an agent in a
Docker container (or another sandbox) that reaches the host's whiteboxed — through
`--network host`, a port forward, or a shared loopback — could create directories and
place files anywhere the user can write **on the host**, outside its sandbox. The
content was limited (diagrams and generated text, no code execution), but it was a
write primitive across the sandbox boundary.

Fixed in `5c1a625`:

- AI exports go only into folders the user allowed for this session. An export into a
  new folder fails and the window asks **Allow for this session / Don't allow**; the
  AI dialog lists, revokes and pre-allows folders. Nothing is remembered after quit.
- The folder is resolved before the check: `..` is refused, and symlinked folders are
  followed, so a link inside an allowed folder that points outside is not allowed.
- No exported file is written through a symbolic link (also for menu exports).

Tests: `export_asks_once_per_folder_and_stays_inside_it`,
`export_never_follows_symlinks_out_of_the_allowed_folder`,
`an_ai_export_request_is_answered_in_the_window`.

### S2 A box path with special characters crashed the app — High, fixed

Resolving `"Web Shop/Orders"` cut the original string at a position computed on its
lowercased copy. Lowercasing changes byte lengths for some characters (`İ` becomes two
characters), so a path such as `"é/İİ"` cut inside a UTF-8 character and panicked.
**One tool call could close the app**, losing unsaved work. Found by the clippy
`string_slice` scan, reproduced by a test, fixed in `8acee56` (split only at `/` in the
original text).

### S3 Typed text could inject markup into exports — Medium, fixed

Box names, responsibilities, motivations and relation texts went into the exported
AsciiDoc and Markdown unchanged. A text such as `include::/etc/passwd[]` or
`+++<script>…+++` would act when the documentation is rendered (Asciidoctor in unsafe
mode reads the included file; passthroughs emit raw HTML). In Markdown, `<script>`,
`<img onerror>` and `[x](javascript:…)` reached the output. Since an AI writes these
texts, this mattered. Fixed in `6b08227`: AsciiDoc output escapes preprocessor
directives, `pass:`, `+` and attribute references; Markdown output escapes `<`, `>`,
`[` and `]`. Covered by a test with each payload.

### S4 Browser pages were not refused by Origin — Medium, fixed

The endpoint already accepted only loopback `Host` names (no DNS rebinding) and
requests carrying the token. Origin validation was off by default in the MCP library.
Fixed in `cedf55a`: every request with an `Origin` header (that is, from a browser) is
refused with 403, even with the right token. MCP clients do not send one; the official
Node MCP client was re-tested against the hardened server.

### S5 An AI could flood the model — Medium, fixed

Nothing limited how much an AI could add. Hundreds of thousands of relations, boxes
spread over a huge grid, or megabyte names would make every redraw recompute the
layout for seconds (the window freezes), and a cell at `i32::MAX` overflowed when a
neighbour was added. A PNG export of a very wide diagram at scale 2 could allocate
gigabytes. Fixed in `ad1bdf2`, `3cfd15e` and `0512190`:

| Limit                      | Value                   |
|----------------------------|-------------------------|
| Name and tag length        | 120 characters          |
| Text length                | 10,000 characters       |
| Boxes per diagram          | 250                     |
| Relations per diagram      | 1,000                   |
| Grid cells                 | −1,000 … 1,000          |
| Exported PNG, longest side | 8,192 px                |
| `render_diagram` PNG       | 1,500 px                |
| MCP request body           | 4 MiB (library default) |

The same limits are checked when a project file is loaded.

### S6 A bug in a tool call would close the app — Medium, fixed

A panic in any tool ran on the UI thread and ended the process. Tool calls now run
inside `catch_unwind` (`0512190`); changes are made on a copy of the project, so a
panic leaves the project unchanged and the AI gets "internal error; nothing was
changed". Unsaved work is additionally kept by the recovery file.

### S7 Control characters broke exports — Low, fixed

A name containing e.g. U+0000 produced invalid XML, so SVG/PNG export and
`render_diagram` failed for the whole diagram. Names and tags now reject control
characters; texts allow only line breaks and tabs (`ad1bdf2`).

### S8 Menu entry temp file through a symlink — Low, fixed

On Linux the app writes its menu entry through `~/.local/share/applications/
.whiteboxed-<pid>.tmp`, opened with `create`, which follows an existing symlink. Now
any leftover is removed and the file is created with `create_new` (`d6fed37`).
Project saves already used `create_new` and an atomic rename.

### S9 Recovery files readable by other users — Low, fixed

Unsaved work in `whiteboxed/recovery` was created with default permissions. The
directory is now `0700` on Unix (`3cfd15e`). The token file `ai.yaml` was already
`0600`. On Windows both rely on the per-user profile directory's ACL.

### S10 Project files are parsed by a C-translated YAML parser — Low, accepted

`serde_yaml_ng` uses `unsafe-libyaml`, a mechanical translation of libyaml with about
14,500 unsafe expressions (the largest share in the tree). It parses every project
file, including files from other people's repositories. There is no known advisory;
the parser limits recursion and alias expansion ("billion laughs"), and the model
checks every reference after loading. An AI cannot make whiteboxed open a file.
Recommendation: move to a parser without unsafe code when one with comparable
stability is available.

### S11 `ttf-parser` is unmaintained — Low, accepted

RUSTSEC-2026-0192 (informational, not a vulnerability). whiteboxed uses it only to
measure its own embedded font; `resvg` depends on it too. Accepted in
`.cargo/audit.toml` and `deny.toml` with this reason; revisit when `resvg` moves on.

### S12 The token can leak through the client side — Low, documented

On the whiteboxed side the token has 244 random bits, is compared in constant time,
lives only in `ai.yaml` (never in a project) and the endpoint is off by default. It
can still leak where the user puts it:

- the AI client's configuration (`~/.claude.json`) stores it in plain text;
- `claude mcp add --scope project` writes it into `.mcp.json` in the repository, and
  from there into git — the workflow docs warn against it;
- the `?token=` form ends up in shell history and logs; prefer the header;
- **Copy command** puts it on the clipboard.

Anyone with the token and access to the port can change the model and export into
folders the user allowed. **Generate new token**
in the AI dialog revokes it.

### S13 CI actions were pinned by tag — Low, fixed

Workflows used third-party actions by version tag or branch
(`dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`,
`taiki-e/install-action@v2`, `actions/*@v4`). Whoever controls such a tag could run
code in the release job, which holds `CARGO_REGISTRY_TOKEN` and `CHANNEL_PAT`. Every
action is now pinned to a full commit SHA, with the version it was resolved from in a
comment (`dtolnay/rust-toolchain` has no releases; it is pinned to the head of its
`stable` branch and given `toolchain: stable` explicitly). Updating an action is now a
deliberate change: resolve the new tag with
`gh api repos/<owner>/<repo>/commits/<tag> --jq .sha` and replace the SHA. The
`CHANNEL_PAT` is limited to the tap and bucket repositories, and pull requests from
forks get no secrets.

### S15 AI access can listen beyond this computer — Medium, mitigated

Added in 0.2.0 so that an AI client in a Docker container can connect. The AI dialog
offers three choices under **Who may connect**:

- **This computer only** (default): binds `127.0.0.1`, accepts the `Host` names
  `localhost`, `127.0.0.1` and `::1`. Nothing changed against 0.1.0.
- **Docker containers on this computer**: on Linux binds only the address of the
  `docker0` bridge (read from `/proc/net/route` and `/proc/net/fib_trie`, no new
  dependency, no `unsafe`); with Docker Desktop (macOS, Windows), which forwards
  `host.docker.internal` to the host's loopback, it stays on `127.0.0.1`. Accepted
  `Host` names: the bridge address and `host.docker.internal`. Every container on the
  bridge can reach the port, so the token is the barrier.
- **Other address**: binds the typed IP address. Every machine that reaches it can
  try the token; the dialog says so in orange.

Before 0.2.0 a container that reached the port (host networking or a forward) was
still refused, because its `Host: host.docker.internal` was not on the allowlist.
That is why the Docker choice exists and why loopback mode still refuses that name.

Mitigations: the token (244 bits) is required in every mode; browser requests are
refused by `Origin` in every mode; the `Host` allowlist is never empty, so DNS
rebinding stays blocked; exports still need the user's approval per folder (S1); the
choice is stored per user (`ai.yaml`), never in a project, and access is off at every
start. Tests: `src/mcp/listen.rs` (bridge lookup, allowlists per mode) and
`tests/mcp.rs` (`host.docker.internal` refused in loopback mode, accepted in Docker
mode, other hosts refused).

Residual risk: on "Other address" the endpoint speaks plain HTTP, so the token
crosses the network unencrypted. Use it only on a network you trust, or tunnel it.

### S16 `batch` could hide a forbidden call — Low, fixed

The `batch` tool (0.2.0) runs other tools in one undo step. It refuses before running
anything when a step is `batch` (no nesting, no unbounded recursion), `export_docs`
(writes files, needs per-folder approval that a rollback cannot take back) or
`render_diagram`. At most 200 steps. Every step goes through the same handler and
validation as a single call; a failing step rolls the model, the undo stack and the
last-AI note back to the state before the batch (tests in `tests/api.rs`).

### S14 Many MCP sessions use memory — Info, accepted

Each initialised MCP session lives in memory until it ends. Only token holders can
open sessions, so this is not reachable without the token.

## Verified safe

- The endpoint binds `127.0.0.1` unless the user chooses otherwise (S15) and is off
  until switched on; it is off again after every start.
- Requests without the token, with a wrong token, with a browser `Origin`, or with a
  `Host` outside the chosen mode's allowlist are refused (tests in `tests/mcp.rs`).
- Calls that time out or arrive while AI access is turned off are never run later
  (`8b20196`); turning access off answers waiting calls and frees the port at once.
- Every tool call goes through the same validation as the GUI and is one undo step;
  a refused call changes nothing.
- SVG output escapes `& < > "`; text in the canvas is drawn, never interpreted.
- The HTML export (0.2.0) escapes every name and text (`& < > " '`) and loads
  nothing from outside: the font is embedded, the only script is its own, fixed one
  (tests in `tests/html.rs`). The AI's `export_docs` writes it on request
  (`html: true`), under the same per-folder approval (S1).
- Saves and recovery writes go through a new temporary file and an atomic rename.
- whiteboxed forbids `unsafe` code (`[lints.rust]` in `Cargo.toml`).

## Tool results

| Tool                            | Version | Result                                                              |
|---------------------------------|---------|---------------------------------------------------------------------|
| `cargo audit`                   | 0.22.2  | 0 vulnerabilities in 508 crates; 1 warning (S11)                    |
| `cargo deny check`              | 0.20.2  | advisories, bans, licences, sources ok                              |
| `cargo geiger`                  | 0.13.0  | whiteboxed: 0 unsafe; dependencies: 103,863 unsafe expressions used |
| `cargo clippy` restriction scan | 1.98.1  | 153 hits reviewed; one real bug (S2)                                |

### cargo geiger

Unsafe code sits where it is expected: GPU and window-system bindings, SIMD and the
async runtime. Largest users (expressions used by the build):

| Crate          | Unsafe expressions | Used for                   |
|----------------|--------------------|----------------------------|
| unsafe-libyaml | 14,491             | YAML parsing (S10)         |
| ash            | 14,082             | Vulkan bindings            |
| wgpu-hal       | 10,949             | GPU abstraction            |
| glow           | 10,297             | OpenGL bindings            |
| fearless_simd  | 8,218              | SIMD for rendering         |
| moxcms         | 5,538              | Colour management (images) |
| wgpu-core      | 3,205              | GPU                        |
| rustix         | 2,737              | System calls               |
| tokio          | 2,277              | Async runtime (MCP server) |
| memchr         | 1,992              | Byte search                |

### Clippy restriction scan

Run with `arithmetic_side_effects`, `indexing_slicing`, `string_slice`,
`cast_possible_truncation`, `cast_sign_loss`, `unwrap_used`, `expect_used`, `panic`,
`unreachable`, `todo`, `exit`, `mem_forget`, `dbg_macro` and `print_stdout`:

| Lint                                         | Hits | Review                                                     |
|----------------------------------------------|------|------------------------------------------------------------|
| `arithmetic_side_effects`                    | 77   | Grid and layout arithmetic, bounded by the S5 limits       |
| `indexing_slicing`                           | 69   | Layout tables indexed by construction                      |
| `string_slice`                               | 2    | One real bug (S2); the other is guarded (ASCII colour hex) |
| `cast_possible_truncation`, `cast_sign_loss` | 5    | Image sizes; Rust casts saturate, and S5 caps them         |
| all others                                   | 0    | No `unwrap`, `expect`, `panic!`, `todo!` or debug output   |

These scan lints are not enforced in CI (most hits are layout arithmetic).

### Lint policy (enforced)

The same policy as sanshain-service, in `Cargo.toml` `[lints]` and `clippy.toml`, and
enforced by `cargo clippy --all-targets --all-features -- -D warnings` in CI:

| Lint                                  | Level  | Why                                 |
|---------------------------------------|--------|-------------------------------------|
| `unsafe_code`                         | forbid | Memory safety left to the compiler  |
| `unwrap_used`, `expect_used`, `panic` | deny   | No panics from input (tests exempt) |
| `todo`, `unimplemented`, `dbg_macro`  | deny   | No unfinished or debug code shipped |
| `allow_attributes`                    | deny   | Lints are fixed, never silenced     |

## In CI

- **Security** workflow (`.github/workflows/security.yml`), on every push, pull request
  and daily at 05:17 UTC: `cargo audit` fails the run on any vulnerability advisory
  for a crate in `Cargo.lock`; `cargo deny check` fails on advisories, disallowed
  licences, unknown registries or git sources. Verified: a crate with a known CVE
  (`smallvec 1.6.0`, RUSTSEC-2021-0003) makes `cargo audit` exit with an error.
- The **Release** gate runs both before anything is built or published.
- Accepted advisories are listed with their reason in `.cargo/audit.toml` and
  `deny.toml`; only informational ones are accepted.

## Re-running the audit

```sh
cargo install --locked cargo-audit cargo-deny cargo-geiger
cargo audit
cargo deny check
cargo geiger --all-features
cargo clippy --lib --bins --all-features -- \
  -W clippy::arithmetic_side_effects -W clippy::indexing_slicing -W clippy::string_slice \
  -W clippy::cast_possible_truncation -W clippy::cast_sign_loss -W clippy::unwrap_used \
  -W clippy::expect_used -W clippy::panic
cargo test
```
