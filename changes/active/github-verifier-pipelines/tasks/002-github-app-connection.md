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
