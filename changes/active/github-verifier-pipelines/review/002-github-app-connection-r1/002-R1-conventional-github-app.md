---
id: TASK-002-R1
kind: remediation
status: proposed
parent_task: TASK-002
approved_spec_revision: 5
findings: [FIND-002-1, FIND-002-2, FIND-002-3, FIND-002-4, FIND-002-5, FIND-002-6, FIND-002-8, FIND-002-9, FIND-002-10, FIND-002-11, FIND-002-12, FIND-002-13, FIND-002-15]
---

# Complete the conventional GitHub App connection

Implement this packet with `$wyrd-implement` against the current working tree.
The user approved revision 5 to use GitHub.com’s documented App installation
and OAuth flow. The specification decision raised by the original review is
resolved. This packet is implementation-ready under that scope.

## Authority and immutable review context

- Approved spec is [revision 5](../../spec.md), with [contracts](../../contracts.md)
  and [App design](../../design/approved-runtime-and-github.md).
- Original task is [002 GitHub App connection](../../tasks/002-github-app-connection.md).
- Reviewed base is `395274d58bf76a2075f245ca3a5c1bd044925027`.
- Reviewed candidate is `3dd1555a0f40117ee512ecaaff1f5b51845ef3b6`.
- Preserve [the original verdict](verdict.md), [validated ledger](findings-validation.md),
  all specialist reports, and [independent verification](verification.md) as
  evidence for revision 4. [Current disposition](scope-resolution.md) records
  the subsequent human decision. No implementation correction is claimed yet.

Paths below refer to the reviewed candidate. `service.rs`, `app.rs` and
`routes.rs` belong to `crates/wyrd/wyrd-server/src/components/github/`.
`slots.rs` means `crates/wyrd/wyrd-sql/src/queries/github_installations.rs`.
The server integration owner is
`crates/wyrd/wyrd-server/tests/integration/pg_github_connection.rs`.

## GitHub.com scope and finding disposition

Production uses `https://github.com` and `https://api.github.com`. Keep local
provider origins for development mocks. Apply this scope through the existing
GitHub configuration validation, configuration tests and operator guide. Use
GitHub.com’s documented setup and OAuth callback with ordinary state, PKCE and
installation confirmation. The existing verified callback already supplies
that workflow. Add no provider discovery, capability negotiation, alternative
authorization protocol or additional user confirmation step.

Only the active IDs in this packet require work. The removed Enterprise Server
scope is historical context in `scope-resolution.md` and supplies no additional
implementation requirements. Use the existing GitHub.com callback and mock to
prove ordinary connection and refusal of a code substituted from another
transaction.

FIND-002-2 is limited to existing saved association and OAuth transaction
settings. It adds comparisons against persisted fields, with no provider
management or cross-host test matrix.

## Diagnosis, selected correction and closure proof

### FIND-002-1 — Credential handoff after revocation

REQ-001, AC-001 and O19/O22/O23 require access loss to fence source use.
`service.rs:635–668` commits an unlocked slot snapshot before provider IO and
returns the token directly. Disconnect can release that slot and another tenant
can claim it while the response waits. The old tenant then receives usable
authority. Sequential refusal assertions never overlap these operations.

Make credential handoff conditional on the same active slot revision after
provider IO using the existing tenant slot owner. Discard a stale provider
result. Refuse released, replaced, suspended or deselected access. If the revision changed, return the existing typed stale-connection refusal
after discarding the result. A normal new request checks current authority.
Prove a new request for an unaffected repository succeeds. A revision changed by release/reclaim
cannot authorize handing out the old result. End SQL transactions before
provider waits. This is the shared token owner’s trust boundary and needs no
new queue or durable state.

Hold an existing mock mint response, commit disconnect, then release the
response and assert the old request returns no credential. Exercise selection
loss and an unrelated-repository control. Update the inaccurate serialization
claim at `service.rs:325–329` alongside the correction.

### FIND-002-2 — Saved identity ignored by current configuration

INV-001 and O6/O18/O19 require saved authority to describe the provider used.
The slot persists host, and login state persists issuer, client ID and callback
URL. `service.rs:177–198,369–393,439–451,640–668` uses current settings without
checking those saved values. Configuration replacement can reinterpret old
state or send an old authorization code to a different configured client.

Check saved slot identity before projecting active access or requesting a
token, preserving authorized release of stale associations. Compare saved
issuer, OAuth client and callback with current configuration at setup and
callback before redirect, consumption or code exchange. Reuse current typed
transaction refusal and existing persisted fields. Prove a mismatched saved
slot yields no active authority or token request, and a transaction begun under
old settings cannot redirect or exchange under replacement settings. Normal
release and a fresh legitimate connection must succeed. No cross-host product
workflow is needed.

### FIND-002-3 — Invalid signing key accepted at startup

O3/O16 require typed configuration failure for invalid present App settings.
`config.rs:2439–2453` and `app.rs:148–156` parse an RSA encoding key, but the
installed `jsonwebtoken` decoder accepts the fixture’s public PEM. Actual
RS256 signing fails only on the first provider request, misreported as provider
unavailability.

Validate RS256 signing through the installed signer during configuration
resolution and let the App consume the validated key. Remove duplicate
decoding. Use the checked-in public half to assert a field-specific typed
configuration error, retain the valid private-key case, and verify the signed
App request. Keep signing material out of diagnostics.

### FIND-002-4 — OAuth response media negotiation

O7/O8/O10 require the provider callback to work. `app.rs:221–241` inherits the
REST vendor Accept header and decodes JSON, while the web token endpoint
documents JSON negotiation through `Accept: application/json`. The mock at
`wyrd-testing/src/github.rs:531–555` always responds with JSON and masks this.

Set the documented JSON Accept header at the web exchange and keep the REST
API’s existing headers. Make the existing mock require that negotiation and
drive the normal callback through it. Reuse screened transport and the existing
JSON decoder. A new response parser or GitHub library is unnecessary.

### FIND-002-5 — Suspension delivery order corrupts current access

AC-001 and O21/O22 require synchronized access. `service.rs:587–595` writes the
arriving action, and `slots.rs:69–78` skips equal updates. A newer unsuspend
followed by an older suspend can leave an active installation fenced
indefinitely. Ordered delivery assertions cannot establish recovery.

On either signed suspension transition, use the existing App installation
lookup to obtain current provider state. Apply that observation through the
existing suspension owner conditional on the routed slot identity and revision.
Keep provider IO outside the SQL transaction. If the revision changed during
the lookup, discard the observation and return an existing typed failure so
the delivery can be retried through the ordinary recovery path.
Advance the private fence even for an unchanged successful observation so an
older in-flight observation cannot overwrite it. Preserve public timestamps
when the public state is unchanged. Provider failure supplies no observation.
Keep genuine suspension ownership and ordinary deletion/release semantics.

Prove reverse deliveries and an older paused observation overtaken by a newer
matching observation. Assert the final public state and scoped token decision
match current provider state. Do not add event history or a delivery store.

### FIND-002-6 — Repository removal misses a pending selection

O10/O21/O22 require acknowledged access loss to fence stale callbacks.
`slots.rs:81–96` advances revision only when removed IDs overlap current
selection. With A selected and a callback already having read A+B, a signed
removal of B is acknowledged without a fence, then the callback restores B.
The SQL assertion at `pg_github_installations.rs:482–488` encodes this gap.

Advance the existing private revision on a nonempty removal for the exact
associated host/installation even if selection has no overlap. Subtract only
named repositories and retain public timestamps when selection is unchanged.
Using real SQL, capture the binding for A, remove B, and attempt A+B with the
old binding. Assert stale refusal and A retained. The opposite commit ordering
must remove B normally. Keep the approved associated-installation routing
boundary. No pending claim or event precedence system is authorized.

### FIND-002-8 — Browser setup response permits caching

The security posture’s credential response rule applies to
`routes.rs:189–207`, which returns the binding cookie and redirect state
without `Cache-Control: no-store`. The router supplies no equivalent directive.
Use the existing `auth/oauth.rs:140–151` response owner, as browser login does.
Assert no-store, the binding cookie and the redirect together on the served
setup response. Cookie flags alone cannot close the cache-policy gap.

### FIND-002-9 — Unearned local HMAC implementation

The repository reuse rule applies to `wyrd-crypt/src/lib.rs:449–480`, which
implements HMAC key normalization and padding despite resolved RustCrypto
`hmac` 0.12.1. Keep the existing shared signing/verification API and implement
it through that installed primitive and its constant-time full-tag verifier.
Remove the direct `subtle` dependency if unused. Preserve the independent RFC
4231 short/long-key and wrong/truncated-tag assertions and signed webhook proof.
No new crypto package or provider dependency is needed.

### FIND-002-10 — Required Rust documentation is incomplete

The explicit changed-item rustdoc gate covers the inventory in
`standards-review.md`, REP-GH-01, including new module declarations, SQL test
constant and associated type, fixture panic contracts and callback cancellation
after state consumption. `slots.rs:169–171` also calls an uncommitted
caller-owned transaction result committed.

Complete the following inventory while making these corrections and relocating
the tests.

- Document new module declarations in `wyrd-client/src/lib.rs:22`,
  `wyrd-spec/src/lib.rs:19`, `wyrd-cli/src/lib.rs:19`, server
  `components/mod.rs:7`, `components/github/mod.rs:10–12`, `mcp/mod.rs:70`,
  SQL `queries/mod.rs:16`, `wyrd-testing/src/lib.rs:7`, Python `src/lib.rs:16`,
  both TypeScript native `lib.rs` files and the CLI/MCP/server/SQL test registrations.
- Document SQL test `HOST` at `pg_github_installations.rs:33` and
  `TryFrom::Error` at `queries/auth/login_state.rs:372`.
- Add actual panic contracts for CLI helpers at `github_journey.rs:21,35,46,56`,
  server test helpers at `pg_github_connection.rs:24,33`, config fixture helper
  at `config.rs:6790`, the OpenAPI test at `pg_openapi_contract.rs:300`, and
  Rust SDK helpers at `github_connection.rs:24,33,46,69,88,114,122`.

Document caller-owned commit, spent authorization state after failure,
and actual credential fencing. Include the changed test declarations. Inspect
the complete changed-item set and run the narrow format/lint checks. Batch this
work with behavior fixes.

### FIND-002-11 — SQL tests silently pass without PostgreSQL

SQL directory rules forbid silent environment skips. Every new test in
`pg_github_installations.rs` returns before `PgFixture::start` when its local
environment helper finds no database URL. A miswired lane reports success
without testing uniqueness, isolation or revocation.

Delete that guard and rely on the existing fixture’s visible failure. Run the
focused SQL cases under repository-managed PostgreSQL and one representative
direct selection without the environment. The latter must fail visibly.

### FIND-002-12 — Real-server tests lack required source placement

The CLI `tests/github_journey.rs` and Rust SDK integration
`github_connection.rs` run server/PostgreSQL tests at top level without the
required `pg_tests` module or `pg_*` filename. Use the existing source marker
convention, update target registration and affected `mise.toml` selectors, and
prove the same cases are discovered and pass. Keep existing binaries and lane
membership.

### FIND-002-13 — Published setup commands cannot be followed

O30/AC-011 and docs publishing rules apply to
`docs/src/content/docs/operate/connect-github.md:48–67`. Bare angle-bracket
assignments are shell redirections. The CLI journey injects environment in Rust
and never executes the published setup snippets, so its green result misses
this failure.

Use valid substitutable values and the existing local-example convention for
operator setup and tenant CLI commands. Drive that minimal example with the
existing mock credentials and real-server CLI journey, then verify the guide
and generated docs through their owners. Update provider scope to GitHub.com.
The permissions/events and served URL assertions already pass and carry forward.

### FIND-002-15 — Exact named proof commands are incomplete

The task records module filters and abbreviated commands for individually
named config, SQL, server, CLI, SDK and in-process callback tests. The repository
requires executable exact selectors for named proof. Independent verification
already supplies exact HMAC/OpenAPI selectors, real-provider OIDC evidence and
TypeScript format/lint/type results.

Record and run only missing exact selectors, grouped under the existing
repository wrappers and target. Use final names after test placement changes.
Carry forward already valid proof. Preserve the disclosed missing historical
RED; record genuine focused failure and success for new behavioral repairs.

## Acceptance and execution boundaries

Each section’s stated observation is the acceptance criterion for its finding.
All thirteen active IDs must have source correction or the specified evidence
attached. Batch the HMAC reuse, Rust documentation, SQL fixture, test placement
and exact command recording with the relevant behavior changes. These are
small repository-required corrections, with no separate implementation or
review round for each item.

The historical diagnosis item is satisfied by the independent source tracing
already in `findings-validation.md`, FIND-002-14, and the behavior and standards
reports. They trace the Card fixture, authentication header and catalog causes.
Reference that evidence in the task record and retain the lack of original
before-edit traces as a historical limit. No new diagnostic agent or report is
required. This is an amended disposition of the earlier remediation demand.

Preserve tenant RLS, inverse uniqueness, OIDC behavior, current permission/audit
owners, transient/redacted credentials, committed receipts, unrelated repository
access and existing language projections. Use the dedicated table and existing
login state. Task 006 adds actual PR-linked queued work to the same fence.
Keep UI, new queues/tables, capability negotiation, alternate provider protocols
and unrelated refactors outside this packet.

## Focused verification and handoff

Record exact selectors for new regression tests once they exist. Use the
existing server/mock integration for credential and webhook interleavings, SQL
integration for revision/claim behavior, config unit tests for provider/key
validation, and the existing CLI story for executable guide proof. Reuse the
repository PostgreSQL lifecycle and migration tasks. No new harness is needed.

Run only the changed exact config, SQL, server, CLI and Rust SDK cases. Add the
existing HMAC vector and matching callback/webhook assertions when their owners
change. If callback transport affects projections, run their existing focused
GitHub cases on the shipped surfaces. Run `mise run fmt`, `mise run lints`,
`mise run check:tenant-isolation` and `mise run check:deps` for this write set,
plus `mise run docs:check` for guide/example changes. Run codegen and language
checks only when their contracts or SDK sources change. Full journey suites
and broad aggregates run at integrated change review.

Hand this packet directly to `$wyrd-implement`. Later task review checks the
candidate-to-remediation diff, finding closure and affected consumers, carrying
forward the original reviewed evidence for unchanged behavior.
