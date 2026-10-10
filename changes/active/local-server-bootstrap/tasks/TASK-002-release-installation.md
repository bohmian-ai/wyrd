---
id: TASK-002
spec: SPEC-local-server-bootstrap@2
depends_on: []
maps: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-015, INV-001, INV-004, AC-002, AC-008]
---

## Outcome and Value

An installed `wyrd` CLI obtains and runs the latest stable official server
bundle for its supported host architecture, verifies it, and preserves a
working installation when an update fails.

## Owners, Scope, Consumers, and Prohibited Changes

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
| --- | --- | --- | --- | --- | --- |
| CLI dispatch and package entry | `wyrd_cli::Cli/Command/run_cli_code` | Python `wyrd.cli.run_wyrd_cli`, TypeScript `wyrd/src/cli.ts`, CLI and package tests | No server install command | Extend one Rust command tree and its existing package projections | None |
| Official server bundle | `.github/workflows/release.yml` build/publish server jobs | `verify-release-artifacts.sh`, current Linux matrix and release checksums | macOS bundles and downloader-consumable verified metadata are absent | Extend official release publication | No alternative release channel |
| CLI errors | `WyrdCliError` derive catalog | `run_cli_code` renderer and CLI tests | Download/version failures lack codes | Extend existing error catalog | No parallel error mapper |

Use the existing server bundle shape (binary plus UI). The release installer
may own downloaded artifacts, but must not own server migration, credentials,
or durable application state. Do not add a package-specific downloader,
dependency for a few lines, or silent update to `server dev`.

## Approach

1. Prove release selection and failed-install preservation against a local
   mock release service.
2. Extend the official release workflow to publish the four approved target
   bundles and verification metadata.
3. Add `wyrd server install` to the Rust CLI; select the newest stable
   version and compatible target, verify, install, and report the version.
4. Check CLI/server compatibility before a new binary is eligible to run,
   and project the command through the existing Python and TypeScript entries.

## Ordered Implementation Scenarios

### Scenario 1 — Latest compatible bundle

**Behavior.** The command selects the newest stable versioned release and
correct OS/CPU bundle, reports the version, and refuses an incompatible
client/server contract before use. (REQ-001, REQ-002, REQ-004)

**RED.** Add `server_install_selects_latest_stable` in the existing
`wyrd-cli` integration target; it fails because the command is absent. Run
`mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=server_install_selects_latest_stable)'`.

**GREEN.** Extend the shared CLI command path and official artifact
publication; rerun the exact test.

**REFACTOR.** Reuse the release bundle/version vocabulary already published
by CI and keep package entrypoints as projections of one Rust command.

### Scenario 2 — Failed or interrupted installation

**Behavior.** Missing target or provenance, changed bytes, failed transfer,
and interrupted extraction fail closed; a previous verified installation
remains executable. (REQ-003, REQ-015, INV-004)

**RED.** Add `server_install_preserves_previous_on_failure` in the same
confirmed CLI target; it fails on the first unhandled refusal. Run
`mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=server_install_preserves_previous_on_failure)'`.

**GREEN.** Complete verified, failure-safe installation and rerun both exact
tests.

**REFACTOR.** Consolidate only duplicated verification or replacement logic
while preserving each refusal and the prior installation.

## Acceptance Criteria

Python and TypeScript installed CLI entries expose the same command. Official
release publication has all four approved bundles and verification data.
No unverified or incompatible downloaded binary can run; updates are explicit
and failure-safe. Mock release tests need no live GitHub access.

## Expected Write Set and Consumer Closure

Likely: `crates/wyrd/wyrd-cli`, `.github/workflows/release.yml` and its
artifact verification, existing Python/TypeScript CLI projection tests, CLI
errors and generated error declarations when changed, and release/install
docs. Paths are guidance, not an implementation allowlist.

## Verification and Evidence

Run both exact scenario commands, then `mise run test:wyrd`,
`mise run py:test:cli:unit`, `mise run ts:test:unit`,
`mise run codegen:check` if public error declarations change,
`mise run docs:check`, `mise run fmt`, `mise run lints`, and
`git diff --check`. Validate macOS/Linux artifact names, bundle contents,
and provenance in the release-workflow dry run; this static CI proof has no
manufactured RED.

## Docsite Rebuild Update

After implementation and verification pass, update the developer docsite
rebuild so developers can use what this task delivered. The rebuild lives in
the `wyrd-doc-site` worktree under the `developer-docsite-rebuild` change
packet. Its spec sets the page map, and its tasks set the page rules. Use
`$human-tech-docs`, keep pages `draft: true` with an accurate `status`, and
describe only behavior this task delivered and verified. Keep the development
setup minimal: put production detail in Operate, not Get started. Run every
documented command against the delivered build, then run the docsite's
`docs:check:commands`, `docs:linkcheck`, `docs:build`, and `docs:a11y`.
Record the pages changed and the checks run in Implementation Evidence.

Pages:

- Get started → *Set up Wyrd*: install the server with the CLI
  (`wyrd server install`) instead of building it from a checkout. Keep the
  checkout build only where a contributor needs it.
- Operate → *Troubleshoot a deployment*: failed download or verification, an
  unsupported host, and a failed update that keeps the working installation.

## Material Stop Conditions

Stop for spec revision if current release provenance cannot establish the
approved trust contract, or if compatibility cannot be decided before
migration without a new public compatibility rule.

## Authority Links

`changes/active/local-server-bootstrap/spec.md` revision 2;
`AGENTS.md` §§2, 9, 11; `architecture/operations/deployment-and-release.md`;
`architecture/references/languages/testing-workflows.md`.
