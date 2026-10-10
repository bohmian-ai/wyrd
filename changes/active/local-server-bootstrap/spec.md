---
id: SPEC-local-server-bootstrap
revision: 4
status: approved
---

# CLI-driven local server and MCP setup

## Objective and user value

A developer installs Wyrd without a checkout or Rust toolchain, obtains the
latest official server release for their machine, and starts a usable local
deployment from the `wyrd` CLI. First use migrates the database, starts the
server, provisions the initial tenant credential, saves client configuration,
and offers to connect detected coding agents to Wyrd's MCP endpoint. A new
CLI or SDK process can immediately use the deployment, and a selected MCP
host can ask Wyrd a question.

MCP setup also works independently for developers connecting to an externally
hosted Wyrd server. It uses existing Wyrd client credentials without starting
a local server or changing the global endpoint to localhost.

## Developer journeys

### First local deployment

1. Install the Python package (`pip install wyrd`) or TypeScript package
   (`npm install @wyrd/sdk`); each exposes the same `wyrd` CLI.
2. Provide external PostgreSQL, set `WYRD_DATABASE_URL`, then run
   `wyrd server dev --tenant local`. Storage defaults to a local directory.
3. If no server release is installed, the CLI downloads and verifies the
   latest stable official release for the local OS and CPU architecture.
   `wyrd server install` performs this explicitly and requests later updates.
4. The CLI migrates, starts the server, waits for readiness, creates the
   platform root and tenant, saves the tenant credential and localhost endpoint
   in Wyrd's existing user configuration, and offers detected MCP hosts for
   selection. It remains attached to the server until interrupted.
5. In a fresh process, use `wyrd` or an SDK without exporting a client URL
   or API key. A selected MCP host discovers and calls a Wyrd read tool using
   the same credential resolution.

### Existing or externally hosted deployment

A developer selects a deployed server and tenant through the existing
`wyrd auth login --server <url> --tenant <slug>` flow, or uses another
credential source already supported by `wyrd-client`. They run
`wyrd mcp install --server <url>` to select detected local MCP hosts. This
does not start a server or rewrite the global client endpoint. The host
connection retains that server URL and resolves credentials when used. SDK
users select the deployment through existing explicit or ambient client
configuration; this change adds no login or credential format.

## Requirements and observable behavior

### Release installation and server lifecycle

- **REQ-001 — Installed CLI.** Installed Python and TypeScript packages
  expose `wyrd server` and `wyrd mcp` through the same Rust CLI contract.
  Local setup needs no source checkout or Rust toolchain.
- **REQ-002 — Latest official release.** `wyrd server install` resolves the
  newest published stable, non-draft Wyrd GitHub release by version, selects
  its server bundle for the detected OS and CPU architecture, and reports its
  version. It does not silently select a prerelease, older release, or another
  architecture. Supported local-server targets are `aarch64-apple-darwin`,
  `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, and
  `aarch64-unknown-linux-gnu`.
- **REQ-003 — Verified installation.** Before a downloaded server can run,
  the CLI verifies the bundle against the release-published integrity and
  provenance metadata required by Wyrd's release contract. Missing or
  mismatched metadata, failed transfer, or incomplete extraction fails closed.
  A failed replacement leaves the prior verified installation usable and no
  runnable partial installation.
- **REQ-004 — Explicit updates and compatibility.** `wyrd server dev` uses
  the installed server or installs the latest stable release if none exists.
  Later `dev` runs do not silently upgrade it; `wyrd server install`
  requests the newest version. The CLI refuses an incompatible client/server
  contract before migration and explains the required version change.
- **REQ-005 — Local startup.** `wyrd server dev --tenant <slug>` runs the
  installed server's one-off migration, starts the normal server serving path,
  waits for readiness, and provisions through existing server-owned authority.
  It stays in the foreground, forwards interruption, and reports server exit
  or startup failure. Normal `wyrd-server` boot still does not run DDL.
- **REQ-006 — One required database URL.** `WYRD_DATABASE_URL` is the only
  required database URL for migration, serving, `init`, `setup`, and
  `recover-root`. When `WYRD_PLATFORM_DATABASE_URL` is unset, its
  effective value is `WYRD_DATABASE_URL`; when set, its own value is used
  for platform and Iceberg catalog work. The fallback applies to server modes
  generally. Migration runs as the platform login, which must own the
  database objects it creates.
- **REQ-016 — Externally supplied credentials.** Operators create every
  Postgres login and grant its privileges. Wyrd never creates, alters, or
  names a role, and its migrations grant nothing to a named role. Wyrd states
  the privileges each login needs and refuses at boot a login that cannot
  preserve tenant isolation:
  - *Platform login* (`WYRD_PLATFORM_DATABASE_URL`, else
    `WYRD_DATABASE_URL`): runs migrations and owns Wyrd's objects; must not
    be a superuser.
  - *Tenant login* (`WYRD_DATABASE_URL`): must not be a superuser or hold
    `BYPASSRLS`. When a separate platform URL is set, it also must not own
    or be a member of the owner of Wyrd's objects and must not hold
    `TRUNCATE` on them; it needs `USAGE` on Wyrd's schemas and row read/write
    on tenant tables.
  - A local deployment may use one ordinary login that owns the database.
- **REQ-007 — Local inputs.** Startup supplies safe local defaults for inputs
  that can be inferred or fixed locally, including Bifrost memory on macOS.
  PostgreSQL remains an external prerequisite; the developer supplies
  `WYRD_DATABASE_URL`. An unset `WYRD_STORAGE_URL` means the server is running
  locally: storage is `.wyrd/storage` under the working directory, created if
  missing, beside Bifrost's `.wyrd/bifrost`. A set `WYRD_STORAGE_URL` is used
  as given, and a `file://` root it names must already exist.

### First-use state and client configuration

- **REQ-008 — Credential provisioning.** First use creates the platform root,
  the requested tenant, and its administrator credential through existing
  Wyrd authority. The tenant credential is saved to Wyrd's protected
  credential store and becomes usable by the CLI and first-class SDKs without
  an environment export. Platform and tenant credentials remain distinct.
  Plaintext credentials never enter arguments, logs, errors, or MCP host
  configuration; any one-time platform-root disclosure goes only to the
  invoking terminal.
- **REQ-009 — Config location and endpoint.** Local setup persists the
  effective loopback HTTP endpoint in the existing global `config.toml`,
  following `WYRD_CONFIG_HOME`, then `XDG_CONFIG_HOME/wyrd`, then
  `HOME/.config/wyrd`. It preserves unrelated user settings and credentials.
  It does not claim to set `WYRD_SERVER_URL` in the caller's shell. External
  MCP setup does not silently change this global endpoint.
- **REQ-010 — Repeat and partial progress.** Repeating local setup for an
  active tenant neither mints or discloses another credential nor erases user
  config. If a later stage fails after an earlier stage commits, the command
  identifies usable state and a recovery path without silently minting a
  replacement. It reports success only after a fresh client can authenticate
  to the ready server.

### MCP host integration

- **REQ-011 — Independent installation.** `wyrd mcp install` detects
  supported local hosts and asks which to configure in a multi-select prompt.
  Initial variants are Codex CLI/IDE, Claude Code, GitHub Copilot CLI, and
  GitHub Copilot in VS Code. Unavailable variants are reported and not edited.
  Scripts and agents can select named hosts non-interactively; a non-interactive
  run selects no host implicitly.
- **REQ-012 — Minimal host changes.** Only selected host configurations
  change. Installation preserves unrelated entries, identifies an existing
  Wyrd entry before updating it, and is repeatable without duplicates.
  Conflicts and unwritable files are reported per host without corrupting
  other entries. Host files contain no Wyrd credential.
- **REQ-013 — Shared MCP authentication.** A host invokes
  `wyrd mcp proxy` to connect to Wyrd's existing `/mcp` surface. The
  proxy resolves the selected endpoint and authenticates each invocation
  through the existing `wyrd-client` credential chain, token exchange, and
  refresh behavior. It adds no MCP-specific credential source, token cache,
  permission, or server tool contract. An external host connection retains
  the explicit URL given at installation; local setup selects the local URL.
  MCP permissions, tenant identity, errors, and audit remain server-owned.
- **REQ-014 — Usable result.** A selected host can discover Wyrd tools and
  call a read tool against its selected server. Missing or expired credentials,
  unreachable servers, and insufficient permission produce actionable failures
  without disclosing secrets. Writing host config alone is not proof of use.

### Failure reporting

- **REQ-015 — Stable failures.** The CLI provides a stable code and
  actionable cause for missing or invalid inputs, unsupported architecture,
  unavailable release, failed verification, incompatible version, database
  connection or migration failure, server readiness failure, unsafe local
  credential/config storage, and failed host installation. Secrets are
  redacted.

## Constraints and invariants

- **INV-001 — Server authority.** The CLI owns release installation, process
  lifecycle, local configuration, and host integration. The server retains
  migrations, tenant creation, credential issuance, authorization, audit, and
  durable Wyrd state. SDKs and the MCP proxy use the shared `wyrd-client`
  contract rather than duplicating it.
- **INV-002 — Pool-scoped row-level security.** The pool, not the role
  name, selects scope. Every Wyrd and Vala table forces row-level security.
  A tenant-keyed table admits a session whose bound tenant matches the row;
  every table also admits an *operator session*: one whose
  `app.operator` setting is `on` and whose effective role is the owner that
  ran the migration. `TenantConn` binds a tenant and never sets the operator
  flag; `OperatorPool` connections set it at connect time. With one login,
  tenant connections are RLS-scoped and operator connections see across
  tenants. With two logins, the tenant login cannot become an operator
  session even if it sets the flag. Superuser or `BYPASSRLS` tenant logins
  are refused because Postgres exempts them from every policy.
  `SECURITY DEFINER` lookups that must cross tenants declare the operator
  flag on the function itself.
- **INV-003 — Production hardening.** Existing non-database production
  hardening, including signing-key and operator-key requirements, still
  applies. Developer convenience does not bypass authentication or permissions,
  allow remote plaintext transport, or expose management credentials via MCP.
- **INV-004 — Safe local writes.** Credential and config writes use Wyrd's
  existing protections and preserve unrelated user data. Failed or interrupted
  release installation and host edits leave the prior usable state intact.
  Database URLs and credentials are never passed as process arguments.

## Scope and non-goals

In scope: release lookup and verified local installation; four supported
server bundles; `wyrd server install`, `wyrd server dev`,
`wyrd mcp install`, and the MCP proxy; database URL fallback; first-use
credential and client config persistence; detected host integration; docs and
user journeys for local and externally hosted MCP connections.

Out of scope: running PostgreSQL or an object store; Windows local-server binaries;
automatic upgrade of an installed server; changed SDK authentication
contracts; a second Wyrd MCP tool catalog; configuration of undetected hosts;
production fleet deployment or migration orchestration.

## Expensive-to-reverse decisions and authority changes

- **D-001 — One database variable, external credentials.**
  `WYRD_PLATFORM_DATABASE_URL` falls back to `WYRD_DATABASE_URL` in every
  server mode. Wyrd provisions no roles or grants; the named `wyrd_app` and
  `wyrd_platform_admin` roles, `bootstrap/roles.sql`, the BYPASSRLS operator
  role, and role-targeted policies and grants are removed. Tenant isolation is
  pool-scoped RLS (INV-002). Because nothing has shipped, existing migrations
  are rewritten in place. This replaces the mandatory two-named-role rule in
  `SPEC-verified-change-contract` REQ-156 and matching deployment, security,
  and tenancy prose. Per-table command narrowing that depended on named-role
  grants (for example no tenant `DELETE` on Cards, no tenant `INSERT` on Forge
  tasks, an append-only gateway ledger, and column-limited operator reads) is
  deliberately not reproduced: it duplicated the server's own code paths in
  migrations and protected only within a tenant. Wyrd's auth and RBAC model
  is the single permission model; Postgres enforces only tenant isolation and
  table-local invariants, never per-command or per-column permissions. Table-local invariants
  that are not login-specific, such as the audit-staging immutability
  trigger, remain. `SECURITY DEFINER` lookups are executable by every login
  and raise the operator flag for their own body with a save-and-restore
  `set_config` rather than a function `SET` clause, which Postgres reserves
  to superusers for custom settings; they are therefore volatile.
- **D-002 — One CLI across packages.** Python and TypeScript installations
  expose the same Rust-owned `wyrd` behavior. The CLI owns local orchestration;
  server-owned durable operations stay on the server.
- **D-003 — Latest stable, explicit update.** Missing local installation
  resolves the newest stable official release; later `dev` runs do not
  silently change an installed version. Provenance and compatibility are
  checked before execution.
- **D-004 — One MCP credential path.** Host integrations invoke the CLI proxy
  backed by shared-client credential resolution. Each connection retains its
  selected endpoint; host files carry no secret.
- **D-005 — One-off migration.** `wyrd server dev` automates
  `wyrd-server migrate` before serving. Normal server boot retains read-only
  schema verification, preserving `SPEC-verified-change-contract` REQ-157.

Implementing this approved revision requires reconciling the conflicting text in
`SPEC-verified-change-contract`,
`architecture/operations/deployment-and-release.md`,
`architecture/wyrd-security-posture.md`, the tenancy and Postgres layout
foundations, and the self-hosting docs during implementation. Older claims
that two URLs are mandatory, that Wyrd provisions named serving roles, or that
a shared login bypasses RLS do not describe this change.

## Acceptance criteria and evidence

- **AC-001:** From installed Python and TypeScript CLI packages, a fresh
  developer environment with external Postgres completes local
  setup without a checkout. Fresh CLI, Rust SDK, Python SDK, and TypeScript
  SDK processes authenticate and complete representative write/read journeys.
  Evidence: package and real-server user-journey tests.
- **AC-002:** Installation selects the newest stable versioned release and
  correct supported bundle, verifies it before execution, and preserves a
  prior installation on failed update. Missing target, altered metadata or
  bytes, and interrupted transfer fail with stable errors. Evidence: local
  mock release service and release-workflow artifact checks, without live
  GitHub dependence.
- **AC-003:** With only `WYRD_DATABASE_URL` naming an ordinary login that
  owns a fresh database, migration, serving, setup, and platform/catalog
  operations work and tenant connections see only their tenant's rows; with
  both URLs each uses its designated value and the tenant login cannot read
  across tenants even with the operator flag set. Superuser or `BYPASSRLS`
  tenant logins, a platform login that does not own Wyrd's objects, and a
  separate tenant login that owns them are refused at boot. Evidence:
  Postgres integration and server journeys.
- **AC-004:** First local use securely saves the tenant credential and
  loopback endpoint; a fresh SDK and CLI process need no client URL or key
  export. A repeated run does not rotate the credential or overwrite unrelated
  config. Evidence: lifecycle journey and protected-file tests.
- **AC-005:** Interactive detection offers supported hosts and changes only
  those selected. Non-interactive selection, repeats, conflicts, and
  unwritable host config have predictable results. A selected host discovers
  and calls an authenticated Wyrd read tool without a credential in host
  config. Evidence: host-config tests, one real host-to-server MCP journey,
  and contract checks for each supported host variant.
- **AC-006:** A developer selects an externally hosted server, authenticates
  via an existing credential source, installs its MCP connection, and invokes
  a read tool without starting a local server or changing the global
  localhost/default endpoint. Evidence: mock-host and real-server journey.
- **AC-007:** Local startup fails clearly for a missing database URL,
  insufficient migration privilege, unavailable Postgres, migration failure,
  incompatible version, and failed readiness. Partial first-use state is
  recoverable without unintended credential rotation. Evidence: focused CLI
  and server integration tests.
- **AC-008:** Release, config, security, and self-hosting docs agree with the
  delivered commands and URL fallback, and state exactly which privileges the
  operator-created logins need for one-login and two-login deployments.
  Evidence: relevant `mise` checks and documentation review.

## Open material decisions

None.

## Authority and revision history

- `AGENTS.md` §§2, 9, 11; `architecture/wyrd-design.md`;
  `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`;
  `architecture/operations/deployment-and-release.md`.
- `changes/active/verified-change-contract/spec.md` REQ-153–REQ-158,
  especially REQ-156–REQ-157. An approved revision supersedes only the
  conflicts identified above.
- Existing owners: `crates/wyrd/wyrd-cli`, `crates/wyrd/wyrd-server`,
  `crates/shared/wyrd-client`, `crates/wyrd/wyrd-mcp`, and the installed
  Python/TypeScript CLI entry points.

| Revision | Status | Change |
| --- | --- | --- |
| 1 | draft | Initial one-URL, start, setup, and release-binary proposal. |
| 2 | approved | Latest-release local journey, reusable MCP setup, saved client configuration, and explicit database fallback/security decision; approved by the owner. |
| 3 | approved | Externally supplied credentials and pool-scoped RLS replace named serving roles and the shared-admin RLS tradeoff (REQ-016, INV-002, D-001, AC-003, AC-008); approved by the owner. |
| 4 | approved | An unset `WYRD_STORAGE_URL` selects created local storage at `.wyrd/storage` instead of failing (journey step 2, REQ-007, AC-007); approved by the owner. |
