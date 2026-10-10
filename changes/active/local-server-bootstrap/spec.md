---
id: SPEC-local-server-bootstrap
revision: 1
status: draft
---

# One database URL and a CLI-driven local server

## Objective and user value

A developer who installs Wyrd gets a running local server and a usable
credential from the `wyrd` CLI, with one required database variable. Today the
same developer must build `wyrd-server` from a checkout, create three
PostgreSQL logins with `psql`, export two database URLs, run a separate
migration, and run setup from the server binary.

The intended local journey is:

1. Install Wyrd (`pip install wyrd` or `npm install @wyrd/sdk`, which ship the
   `wyrd` CLI).
2. Start PostgreSQL and set `WYRD_DATABASE_URL` and `WYRD_STORAGE_URL`.
3. Start the server with one CLI command, which fetches the server binary for
   the local platform, runs migrations, and serves.
4. Create the first credentials with one CLI command.
5. Set `WYRD_SERVER_URL` and `WYRD_API_KEY`.
6. Run SDK or CLI code.

## Relationship to existing authority

This change overrides, by the owner's explicit decision, the parts of
`SPEC-verified-change-contract` that make two serving logins mandatory and
forbid migration in the serving flow:

- **REQ-156** (exactly two serving logins, each with its own variable) is
  narrowed: `WYRD_PLATFORM_DATABASE_URL` becomes optional.
- **REQ-157** (normal boot never migrates) is narrowed: the CLI start command
  runs migration before it starts the server. `wyrd-server migrate` remains the
  one-off mode for orchestrated deployments.

`SPEC-verified-change-contract` must be revised to reference this change when
this specification is approved.

## Requirements

- **REQ-001 — One required database variable.** `WYRD_DATABASE_URL` is the
  only required database variable for every `wyrd-server` mode, including
  serving, `migrate`, `setup`, `init`, and `recover-root`. When
  `WYRD_PLATFORM_DATABASE_URL` is unset, the server uses the value of
  `WYRD_DATABASE_URL` for platform and Iceberg catalog work. When both are set,
  each is used for its own work, as today.
- **REQ-002 — Start command.** One `wyrd` CLI command starts a local server. It:
  1. uses a `wyrd-server` binary already installed for the CLI's version, or
     downloads the release binary built for the local OS and CPU architecture
     from the Wyrd GitHub release that matches the CLI's version;
  2. runs migrations against `WYRD_DATABASE_URL`;
  3. starts the server in the foreground with the caller's environment.
- **REQ-003 — Setup command.** One `wyrd` CLI command creates the platform root,
  the first tenant, and that tenant's administrator credential against the
  running local server, and prints the credentials once, as
  `wyrd-server setup --tenant <slug>` does today. A retry for an existing tenant
  changes nothing and discloses nothing.
- **REQ-004 — Verified download.** The start command verifies a downloaded
  binary against the checksum published with the same release before running
  it, and refuses to run it on mismatch. A failed or partial download leaves no
  runnable binary behind.
- **REQ-005 — Release artifacts.** Each Wyrd release publishes a `wyrd-server`
  binary and checksum for `aarch64-apple-darwin`, `x86_64-apple-darwin`,
  `x86_64-unknown-linux-gnu`, and `aarch64-unknown-linux-gnu`.
- **REQ-006 — Actionable failures.** The start command fails with a stable error
  that names the cause when: `WYRD_DATABASE_URL` or `WYRD_STORAGE_URL` is unset;
  no release binary exists for the local platform; the download or checksum
  fails; the database is unreachable; or migration fails.
- **REQ-007 — Local defaults.** The start command supplies the local-only
  inputs the server cannot detect itself, such as the Bifrost memory limit on
  macOS, so the developer sets only the variables in step 2.

## Invariants

- **INV-001 — Tenant isolation holds with one login.** When one login serves
  tenant and platform work, row-level security still isolates tenants on every
  tenant request path. The server's boot verification refuses to serve if the
  single login would bypass row-level security for tenant work.
- **INV-002 — Production profile unchanged in strength.** Under
  `APP_ENV=staging|production`, the existing hardening (stable signing key,
  operator key source) still applies. Whether production may run on one login
  is decided by INV-001's check, not by a separate variable.
- **INV-003 — Server owns durable behavior.** The CLI fetches and launches the
  server and calls existing server setup authority. It adds no migration,
  tenancy, or credential logic of its own.
- **INV-004 — No secret in arguments.** Database URLs and credentials reach the
  server and setup through the environment, never through process arguments.

## Scope and non-goals

In scope: optional `WYRD_PLATFORM_DATABASE_URL`, the CLI start and setup
commands, release binaries for the four targets, their tests, the local
user-journey, and the docs that describe local setup.

Not in scope: running PostgreSQL for the developer; changing the production
Kubernetes guides beyond the now-optional variable; Windows binaries; changing
how SDKs authenticate.

## Expensive-to-reverse decisions

- **D-001 Single required variable.** `WYRD_DATABASE_URL` is the only required
  database variable everywhere, by owner decision.
- **D-002 CLI owns the local journey.** Starting and setting up a local server
  are `wyrd` CLI commands, so the journey works from the Python and TypeScript
  packages without a Rust toolchain or checkout.
- **D-003 Version lock.** The CLI downloads only the server release matching its
  own version, never "latest", so client and server contracts match.

## Acceptance criteria and evidence

- **AC-001:** A user-journey test starts from an installed CLI and a fresh
  PostgreSQL with only `WYRD_DATABASE_URL` and `WYRD_STORAGE_URL` set, runs the
  start and setup commands, exports the printed credential, and registers and
  reads a Card through an SDK.
- **AC-002:** With both database variables set, the server uses each for its own
  work; with only `WYRD_DATABASE_URL`, platform and catalog work use it.
  Evidence: integration tests against Postgres.
- **AC-003:** A single login that would bypass row-level security for tenant
  work is refused at boot with a stable error. Evidence: an integration test.
- **AC-004:** A checksum mismatch, a missing platform binary, and each missing
  required variable fail with their stable errors. Evidence: CLI tests against a
  local mock release.
- **AC-005:** The release workflow publishes binaries and checksums for the four
  targets. Evidence: the release workflow's dry run.
- **AC-006:** `mise run docs:check` passes with the local setup page describing
  this journey.

## Open material decisions

- **Command names.** The CLI verbs for start and setup. Proposed:
  `wyrd server start` and `wyrd server setup --tenant <slug>`.

## Authority

- `AGENTS.md` §§2, 9, 11, 15; `architecture/wyrd-design.md`;
  `architecture/wyrd-security-posture.md`.
- `changes/active/verified-change-contract/spec.md` REQ-154–REQ-157, which this
  change narrows.
- Prior local flow: commits `c36ef1355` (`wyrd dev bootstrap`, DSN derivation)
  and `4ae6a1992^` (boot-time migration), replaced by `4ae6a1992`.

## Revision history

| Revision | Status | Change |
| --- | --- | --- |
| 1 | draft | Initial specification from the owner's override. |
