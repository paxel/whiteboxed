# Security policy

## Reporting a vulnerability

Please report a vulnerability in whiteboxed (the binaries, the crate, the install script and
the Homebrew formula and Scoop manifest) privately, through GitHub's
[private vulnerability reporting](https://github.com/paxel/whiteboxed/security/advisories/new)
(the repository's **Security** tab, **Report a vulnerability**). Do not open a public issue,
pull request or discussion for it.

Include what is affected (version, platform), how to reproduce it (a sample project file
helps most), and what an attacker gains, for example a crash or a file written from a crafted
project file or custom look, or more than editing the open model through the AI access token.
The report and its fix are discussed in that advisory, which stays private until a release
with the fix is out.

## What is already checked

Every pull request and every change to the summer branch runs CodeQL, a dependency audit
(osv-scanner) and SonarQube Cloud; OpenSSF Scorecard analyses the summer branch, and the fuzz
targets of the project-file parser and the custom-look SVG check run every week. The CodeQL
and Scorecard findings are the repository's code scanning alerts, SonarQube Cloud's are on its
project page; a report does not need to repeat them. A release is not built while one of them
is open and not accepted in `code-scanning-ignores.toml` with a reason and a date.
[docs/security-audit.md](docs/security-audit.md) is the audit of the app itself.

## Verifying a release

Every release file carries SLSA build provenance, signed with Sigstore by the shared
provenance workflow of paxel/gates:

```sh
gh attestation verify <file> --repo paxel/whiteboxed \
  --signer-workflow paxel/gates/.github/workflows/provenance.yml --source-ref refs/tags/<tag>
```

checks that a file was built from the tagged commit. `whiteboxed-<tag>-provenance.sigstore.json`
on the release holds the same provenance, for a check with `--bundle` without GitHub's API.
