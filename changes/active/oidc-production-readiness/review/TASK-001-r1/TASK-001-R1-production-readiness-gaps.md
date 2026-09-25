---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
requirements: [REQ-004, REQ-005, REQ-016, REQ-017, INV-004, AC-003, AC-007, AC-009]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6, FIND-TASK-001-7, FIND-TASK-001-8, FIND-TASK-001-9, FIND-TASK-001-10, FIND-TASK-001-11, FIND-TASK-001-12, FIND-TASK-001-13]
---

# Close tenant connection production-readiness gaps

## Authority and immutable review subject

- Approved spec: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Reviewed base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Reviewed candidate: `e126cdca7d4bf5bc467279df05cc3e199eb7fdf2`
- Validated ledger: `changes/active/oidc-production-readiness/review/TASK-001-r1/findings-validation.md`

Implement this task through `$wyrd-implement`. A later task review must inspect
the complete cumulative base-to-candidate range, not only the remediation diff.

## Outcome

Make the existing tenant OIDC connection administration implementation satisfy
its approved callback, client-authentication, SSRF, lifecycle cutoff, SQL
capability, audit, sealing-key, contract-validation, documentation, and test-lane
obligations without adding product scope or another owner.

## Issue diagnoses and required corrections

### Provider qualification and callback ownership

`FIND-TASK-001-1` — `probe_callback` in
`crates/wyrd/wyrd-auth/src/connections.rs` accepts any non-error HTTP response.
That does not prove the configured callback is registered, so a tested candidate
can lock out login. Keep the screened redirect-disabled client and make the
non-interactive authorization probe accept only a redirect to the exact
configured callback origin/path that echoes the generated state and contains
either a code or a standard OIDC authorization error. Refuse success pages,
foreign/missing locations, state mismatches, malformed bodies, and all other
responses without stamping.

`FIND-TASK-001-2` — `probe_client_auth` rejects a few authentication failures
but accepts success, malformed data, and unrelated OAuth errors. Those responses
do not prove that the deliberately invalid code reached grant validation after
client authentication. Parse fail-closed and accept exactly OAuth
`invalid_grant`; retain the existing typed refusal for `invalid_client` and
`unauthorized_client`, and reject everything else.

`FIND-TASK-001-3` — runtime login derives its callback from request `Host` and
`X-Forwarded-Proto`, while administration displays and tests the configured
public-origin callback. Delete the header-derived callback from this flow and
use `HumanConnections::callback_url()` for `prepare_login`, refusing when the
deployment callback is absent. Preserve host-based tenant entry resolution.

`FIND-TASK-001-4` — reqwest's system proxy support can bypass
`resolve_to_addrs` pinning in the shared `ScreenedHttp` builder. Add reqwest's
native `no_proxy()` at that one shared owner so every discovery, JWKS, login,
callback, and candidate-probe caller connects to its screened address. Do not
patch callers or add proxy configuration.

### Connection-bound session cutoff

`FIND-TASK-001-5` — callback reads Active before provider IO and never rechecks
before issuance; refresh rotation has no connection provenance. Replacement,
deactivation, or removal can therefore race callback issuance and old refresh
families can continue renewing. Bind login state and every human refresh family
to the exact human-connection id and revision selected at login. In the same
tenant transaction that issues the initial session or a refresh successor, take
the existing tenant connection slot lock and require that exact id/revision to
remain Active. Persist the binding on the initial refresh row and copy it during
rotation. Refuse without inserting a successor when the binding is stale.
Preserve refresh replay containment, current-grant resolution, audit,
tombstones, and five-minute access-token behavior.

### SQL and audit boundaries

`FIND-TASK-001-6` — `HumanConnections` stores a raw `PgPool`, and server state
accepts one to construct it. This defeats the repository's role-separated SQL
capability boundary. Remove the pool field and signature. Existing server
adapters must acquire `TenantConn` through `WyrdPostgres::tenant_conn`, pass it
into the connection workflow, and own commit after success. Keep lifecycle
orchestration on `HumanConnections`; add no allowlist entry or database wrapper.

`FIND-TASK-001-7` — candidate testing evaluates permission once before provider
IO, then fabricates a second Allowed event for the stamp transaction. Preserve
the pre-network evaluation and audit. After provider checks succeed, invoke the
existing decision path again and pass that evaluated event into the locked stamp
transaction. A denied second evaluation must leave the candidate unstamped;
audit append failure must remain fail-closed.

### Repository compliance and test cost

`FIND-TASK-001-8` — candidate-added fields, constants, helpers, and test items
lack mandatory rustdoc, including confirmed examples in `connections.rs`, the
human-connection SQL query module, and `identity_e2e.rs`. Document the intent,
workflow role, invariants, side effects, and applicable error, panic,
cancellation, or partial-progress behavior for every added or materially
modified Rust item. Do not add allowances or placeholder restatements.

`FIND-TASK-001-9` — candidate-added declarations inline qualified types instead
of using the module import block. Move the types identified by the validated
ledger into their existing top-level `use` blocks and use bare names in fields
and signatures. Do not broaden this into unrelated cleanup.

`FIND-TASK-001-10` — the identity journey's list and run commands force
`--all-features`, contrary to repository rules and the original task's exact
command. Remove it from all three commands and retain default-feature filtered
and unfiltered selection behavior. Add no feature knob.

### Sealing-key lifecycle and input contract

`FIND-TASK-001-11` — keyless boot skips rewrap without checking for stored
provider ciphertext, allowing a ready but undecryptable deployment. At the
existing boot rewrap boundary, reuse the existing tenant and platform sealed-
secret query slots to detect ciphertext when no keyring is configured and
return `ServerBootError::SealingKey`. Preserve keyless startup when every store
is secret-free; create no inventory abstraction or duplicate SQL.

`FIND-TASK-001-12` — a K2 replica can report zero old-key rows before a K1
replica writes another secret. The current runbook wrongly treats that first
report as retirement proof. Preserve the idempotent CAS rewrap, but require a
new verification pass only after every serving writer uses K2; only zero
remaining references from that post-roll pass permits K1 removal. Update the
module contract and runbook, and prove a late K1 write is recovered by the
final pass. Do not add leases, coordination state, or another rewrap engine.

`FIND-TASK-001-13` — `Public` plus an explicitly present empty secret is
accepted and silently discarded. Validate presence separately from content:
every `Some` is invalid for `Public`; `SecretBasic` and `SecretPost` require a
nonempty `Some`. Preserve the durable `NULL` invariant for public connections.

## Constraints and preserved behavior

- Preserve the approved wire shapes, route set, stable error catalog, callback
  path, one-Active/one-Candidate model, and 15-minute test stamp.
- Preserve bearer-derived tenancy, RLS, canonical audit storage, transaction
  ownership, workload issuer behavior, legacy migration semantics, and
  redaction of provider and recovery secrets.
- Preserve authorization before provider IO and fail-closed transactional
  mutation audit.
- Preserve connectionless startup when there is no stored ciphertext.
- Reuse `HumanConnections`, `WyrdPostgres`, `TenantConn`, `ScreenedHttp`, the
  existing decision path, refresh-token owner, and CAS rewrap mechanism.
- Do not weaken input validation, security, audit, durability, or test
  assertions to reduce the diff.

## Non-goals

- No UI work, hosted signup, commercial hooks, new authentication method,
  second trust store, compatibility route, or public API redesign.
- No new proxy support, database abstraction, secret inventory service,
  rotation lease/coordinator, or test framework.
- No unrelated refactor or documentation cleanup.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-001-1` | Only an exact state-matching callback redirect proves authorization callback qualification; every ambiguous response leaves the revision untested. |
| `FIND-TASK-001-2` | Only `invalid_grant` proves client authentication; success, malformed data, and all other errors fail closed. |
| `FIND-TASK-001-3` | Real login uses the configured public-origin callback regardless of request Host/scheme headers. |
| `FIND-TASK-001-4` | Ambient proxies cannot observe or reroute screened provider requests. |
| `FIND-TASK-001-5` | Initial issuance and refresh successor creation atomically require the exact connection id/revision to remain Active; cutoff works across replicas. |
| `FIND-TASK-001-6` | The changed connection owner and callers contain no raw `PgPool` field/signature and acquire through `WyrdPostgres`. |
| `FIND-TASK-001-7` | Successful testing records two real evaluations at their boundaries; second denial or audit failure produces no stamp. |
| `FIND-TASK-001-8` | Every added/materially modified Rust item has the required meaningful rustdoc. |
| `FIND-TASK-001-9` | Cited declarations use top-level imports and bare type names. |
| `FIND-TASK-001-10` | Filtered and unfiltered identity journeys list and run without `--all-features`. |
| `FIND-TASK-001-11` | Keyless boot succeeds with no ciphertext and fails before readiness when any provider ciphertext exists. |
| `FIND-TASK-001-12` | A late K1 write is rewrapped after all writers move to K2, and K2-only serving succeeds before K1 retirement. |
| `FIND-TASK-001-13` | Public with any supplied secret and secret methods with empty/missing secrets are rejected; valid counterparts pass. |

## Focused and broader proof

Add or extend the smallest existing tests described by each finding's closure
proof in `findings-validation.md`. The proof must include:

1. focused authorization and token probe cases;
2. configured-callback login initiation with conflicting request headers;
3. an ambient-proxy screened-client regression test;
4. a real-server two-replica old-refresh cutoff and controlled in-flight
   callback race;
5. a denied second test-stamp decision and retained audit-failure rollback;
6. keyless boot with and without stored ciphertext;
7. the late-K1-write/post-roll/K2-only rotation journey; and
8. contract validation for public and secret client-auth inputs.

Run every specifically named Rust test with its exact `mise exec -- cargo
nextest run --locked` selector and repository-managed setup. Then run the
original task's broader lanes: both filtered identity journeys and the
unfiltered identity journey, `test:principals:integration`, `test:sql`,
`test:platform:journey`, `codegen:check`, `check:tenant-isolation`, applicable
SQL/pool/PyO3/unwrap boundaries, `docs:check`, `fmt`, `lints`, and
`git diff --check`. The identity lane must prove those commands use default
features and select exactly the intended tests.
