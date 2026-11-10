---
id: TASK-002
status: proposed
approved_spec_revision: 3
requirements: [REQ-001, REQ-016]
acceptance: [AC-001, AC-011]
invariants: [INV-001, INV-003, INV-006, INV-007]
depends_on: []
---

# Connect the deployment GitHub App

Implement **approved-runtime-and-github.md → Operator App configuration and tenant
installation**, using **contracts.md → GitHub connection**. One operator App
serves SaaS, self-hosted and enterprise deployments through the same code.
Connection grants source access; it does not register a Card or review policy.

## Inputs, owners and selected flow

The operator supplies App ID/slug, configured web/API origins, private key,
webhook secret, OAuth client credentials and exact public URLs through existing
server configuration/secret providers. Tenant callers supply normal authenticated
connection requests and verified repository selections, never keys or arbitrary
hosts. Existing `server/src/config.rs`, `server/src/auth/{browser,callback}.rs`
and SQL `queries/auth/{login_state,human_connections}.rs` own configuration,
browser-bound authorization transaction and connection lifecycles. Extend these
complete protocols and existing control storage for the integration; Operator
credentials are not GitHub installation association.

Implement the single `/v1/integrations/github` connect/read/disconnect flow from
the contracts. Start a one-use actor/tenant/browser-bound transaction with the
existing transaction expiry behavior. Support installation begun from Wyrd or
GitHub. Keep setup and OAuth callback separate: setup's installation ID is a
hint; callback verifies transaction and transient user authorization, revalidates
installation and user repository access, and conditionally associates one
installation with one tenant. Enforce the inverse uniqueness too. OAuth tokens
are transient/redacted. Background work obtains installation tokens and narrowly
authorized execution authority, never a saved personal token.

App key signing, webhook verification, connection callbacks and tenant policy
belong in Wyrd. The independent library supplies generic provider requests only;
deleted historical App branches/commits supply neither code nor an approved pin.
Retain conventional configured GitHub host support. Missing optional App config
makes only this integration unavailable; invalid present config uses typed
validation. Document Contents read, Pull requests write, Checks write and required
PR/source/installation-access events and the actual served setup/callback URLs.

Repository use requires both current installation access and tenant selection.
Synchronize installation removal/suspension/access events. Fence new source IO
and affected unfinished work on access loss; disconnect fences work before
releasing association. Reuse queue cancellation/revocation owners; task 006 adds
its PR-generation records to that same fence. Unrelated repositories continue.

Amend **wyrd-security-posture.md → Trust boundaries / Principal and credential
lifecycle / Outbound credentials** and **wyrd-design.md → Runtime identity** for
the association and credential ownership. Reuse typed permission/audit owners
without prescribing another permission category or claim-management workflow.

## Outputs, surfaces and handoff

Return only the approved redacted connection/access view: configured App/host,
verified installation, connection state, selected accessible repository IDs/names
and safe timestamps. Tasks 003/004/006/009 use this verified identity/access and
installation-token facility; provider metadata alone never confers tenant rights.
The connect transaction receipt follows durable transaction creation; successful
association/disconnect follows committed association/fence state, not URL issue.

Ship typed HTTP, shared Rust client and thin Rust/Python/TypeScript SDK, CLI and
MCP projections here, including safe errors, discoverable reads and scoped
writes. Update operator and tenant setup/diagnosis guides here, with no UI.

## Connected proof

**AC-001:** A real server journey starts both entry paths against a local GitHub
protocol mock, finishes verified selection, reads the public connection and then
disconnects. Assert spoofed installation IDs, consumed/foreign transactions,
foreign tenant, duplicate association and missing/invalid config fail safely;
keys/tokens never appear in view. Remove one repository/suspend/disconnect and
observe only affected existing queued work fenced, association released in the
right order and no Cards/bindings created. Task 006 extends this journey to
actual PR work; do not substitute a fabricated connection there.

**AC-011 (connection portion):** Follow the new CLI/SDK setup guide, inspect the
connection and use the served HTTP/MCP catalog under normal authorization.

Tier 1: SDK story journeys in `sdks/wyrd-sdk-rust` (package `wyrd-ai`), Python
and TypeScript integration trees, plus `wyrd-mcp`'s existing `mcp` target.
Existing runners: `mise run test:rust-sdk:journey`, `mise run py:test:integration`,
`mise run ts:test:integration`, `mise run test:bifrost:journey:mcp`. Tier 2:
conditional association/revocation in SQL's `integration` target via
`mise run test:sql`; tier 3: protocol/signature/config branches in their owners.
Use local provider mocks, real Postgres and WyrdTestServer; no live App keys.
Record new focused commands once tests exist. Run applicable checks from README,
including tenant isolation and OpenAPI; no new PostgreSQL table or UI.

## Obligations

| ID | Source (task line) | Observation | Planned assertion |
|---|---|---|---|
| O1 | "operator supplies App ID/slug, configured web/API origins, private key, webhook secret, OAuth client credentials and exact public URLs through existing server configuration/secret providers" | Every App field parses from server config/env; secrets only from env/file, redacted in Debug | `config` unit test parses and redacts the GitHub section |
| O2 | "Missing optional App config makes only this integration unavailable" | Without App config the server boots, other `/v1` routes work and every GitHub operation refuses with a typed 503 | SDK journey: unconfigured server → typed unavailable error; ordinary read succeeds |
| O3 | "invalid present config uses typed validation" | Partial/invalid App config refuses startup with `ConfigError::Invalid` naming the field | `config` unit test for partial, bad origin, bad key |
| O4 | "Retain conventional configured GitHub host support" | Web and API origins are configurable (GitHub Enterprise Server shape); the view reports the configured host | config unit test + journey view `host` equals mock origin host |
| O5 | "Implement the single `/v1/integrations/github` connect/read/disconnect flow" | `POST /connect`, `GET`, `DELETE` served with typed bodies and in OpenAPI | journey assertions per surface + OpenAPI path assertion |
| O6 | "Start a one-use actor/tenant/browser-bound transaction with the existing transaction expiry behavior"; "connect transaction receipt follows durable transaction creation" | Connect returns the GitHub install URL and expiry only after the login-state row commits; the row binds actor and tenant, then the browser at setup, and expires with `LOGIN_STATE_TTL` | SQL integration: row bound/expiring; journey: receipt then setup works |
| O7 | "Support installation begun from Wyrd" | connect → setup (state) → OAuth authorize → callback associates | journey (Wyrd-begun) |
| O8 | "Support installation begun from ... GitHub" | setup without state, signed-in browser → transaction → OAuth → callback associates | journey (GitHub-begun) |
| O9 | "Keep setup and OAuth callback separate: setup's installation ID is a hint" | setup never associates; only callback does, after verification | journey: after setup, view still not connected |
| O10 | "callback verifies transaction and transient user authorization, revalidates installation and user repository access" | callback exchanges the code, checks the App installation, the user's installation access and user-accessible repositories before associating | journey with mock recording calls; selected repos = user-accessible repos |
| O11 | AC-001 "spoofed installation IDs ... fail safely" | A hint the user cannot access, or another App's installation, is refused and nothing is associated | journey negative |
| O12 | AC-001 "consumed ... transactions" | Replaying a consumed callback state is refused | journey negative |
| O13 | AC-001 "foreign transactions" | A callback state presented by another principal's browser session is refused | journey negative |
| O14 | AC-001 "foreign tenant" | A transaction of tenant A cannot be completed in tenant B's session | journey negative |
| O15 | AC-001 "duplicate association"; "conditionally associates one installation with one tenant. Enforce the inverse uniqueness too" | An installation associated with tenant A is refused for tenant B; a tenant connected to installation X is refused for installation Y; reconnecting X refreshes selection | SQL integration + journey |
| O16 | AC-001 "missing/invalid config fail safely" | = O2/O3 | as O2/O3 |
| O17 | "OAuth tokens are transient/redacted"; AC-001 "keys/tokens never appear in view" | No user/installation token, key or secret in view, errors or Debug; nothing stored | journey: serialized views contain no mock token/key; unit Debug redaction |
| O18 | "Return only the approved redacted connection/access view" | View = host, App id/slug, installation id when verified, state, selected accessible repositories (id, full name), safe timestamps | journey asserts exact view shape |
| O19 | "Repository use requires both current installation access and tenant selection" | Installation token minted only for an associated, unsuspended, selected and still-accessible repository | server integration test of the token facility |
| O20 | "Background work obtains installation tokens and narrowly authorized execution authority, never a saved personal token" | Token facility narrows `repository_ids` and permissions; no user token persisted | integration test inspects mock request body |
| O21 | "Synchronize installation removal/suspension/access events" | Signed `installation` deleted/suspend/unsuspend and `installation_repositories` removed update the connection; bad signature refused | journey via signed webhook + unit HMAC test |
| O22 | "Fence new source IO ... on access loss"; AC-001 "Remove one repository/suspend/disconnect" | After removal/suspension/disconnect the token facility refuses the affected repository; unrelated repositories keep working | integration test |
| O23 | "disconnect fences work before releasing association"; AC-001 "association released in the right order" | Disconnect fences, then releases; afterwards the installation may associate with another tenant | SQL integration + journey |
| O24 | AC-001 "observe only affected existing queued work fenced" | Unrelated queued executions remain unfinished after removal/suspend/disconnect | SQL integration: queued execution survives |
| O25 | "Connection grants source access; it does not register a Card"; AC-001 "no Cards/bindings created" | Card list empty after connect | journey |
| O26 | "Document Contents read, Pull requests write, Checks write and required PR/source/installation-access events and the actual served setup/callback URLs" | Operator guide lists exactly these and the served paths | `mise run docs:check` + doc citation |
| O27 | "Amend wyrd-security-posture.md → Trust boundaries / Principal and credential lifecycle / Outbound credentials and wyrd-design.md → Runtime identity" | Sections describe association and credential ownership | doc citation |
| O28 | "Reuse typed permission/audit owners without prescribing another permission category" | Read/connect/disconnect use existing `identity_connections` read/write; decisions audited | journey: unprivileged caller 403; audit unit |
| O29 | "Ship typed HTTP, shared Rust client and thin Rust/Python/TypeScript SDK, CLI and MCP projections ... safe errors, discoverable reads and scoped writes" | Each surface reads/connects/disconnects; MCP read tool listed for readers, write tools only for writers | per-surface journeys |
| O30 | "Update operator and tenant setup/diagnosis guides here, with no UI"; AC-011 | Guide commands match served CLI/SDK operations | docs:check + CLI journey runs guide commands |
| O31 | Tier 2 "conditional association/revocation in SQL's integration target" | Association conditional on bound slot; uniqueness; release | `pg_github_connections` tests |
| O32 | "Run applicable checks ... including tenant isolation and OpenAPI; no new PostgreSQL table or UI" | Checks pass; migration adds only columns | command output; migration has no `CREATE TABLE` |
