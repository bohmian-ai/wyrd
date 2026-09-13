---
id: TASK-001-R2
kind: remediation
status: in_progress
spec: SPEC-surfaces-oracle-integration
spec_revision: 7
requirements: [REQ-026, REQ-026B, REQ-027, REQ-027C, REQ-028, REQ-029, INV-008, INV-008C, INV-008D, INV-025, AC-005]
depends_on: [TASK-001-R1]
parent_task: TASK-001
remediates:
  - FIND-TASK-001-R1-1
  - FIND-TASK-001-R1-2
  - FIND-TASK-001-R1-3
  - FIND-TASK-001-R1-4
  - FIND-TASK-001-R1-5
  - FIND-TASK-001-R1-6
  - FIND-TASK-001-R1-7
  - FIND-TASK-001-R1-8
  - FIND-TASK-001-R1-9
---

# Close TASK-001-R1 review findings

Route this remediation directly to `$wyrd-implement`. The recommendations below
are the decision-complete result of independent Ponytail validation.

## Authority and immutable subjects

- Approved spec: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-001-integrate-redux-data-plane.md`
- Prior candidate: `40a73817d415e9a1626e6ec7a91edda083e344d3`
- R1 remediation base: `8377fff9f03cc60de4be3e088569e38984382dc4`
- Reviewed R1 candidate: `8e61c03493f5c0ebcc12cdb4a9998df284f7fc8f`
- Verdict and validation: `verdict.md`, `standards-review.md`, and
  `findings-validation.md` in this directory

## Issue diagnoses and required corrections

### 1. Retain system-owner security audit (`FIND-TASK-001-R1-1`)

Unverified peer and tail rejection owners correctly append to the nil
`DataTenantId::SYSTEM_OWNER`, and tenant discovery correctly returns that active
system tenant. Retained publication cannot process it because audit projection,
generic Scribe ingress, and physical tenant binding reject nil. The rows therefore
remain transient forever, hiding required security history and allowing unbounded
system staging growth.

Approved authority already fixes the decision: keep the nil system tenant and
publish its events into the canonical audit table. Reuse `SYSTEM_OWNER`, the Audit
namespace, `AuditLogTable`, the existing publisher, and Scribe's batch fence.
Permit nil only when the internal trusted publication is for
`vala.system.audit_log`; all caller-owned tables, public/non-audit ingress, and
other physical identities continue to reject nil. Do not omit the system tenant,
remap its tenant/hash identity, or globally relax nil validation.

### 2. Consume every Bifrost registration verdict (`FIND-TASK-001-R1-2`)

The server evaluates `bifrost_table:write` and hands an Allowed event to catalog
creation, but catalog validation, physical/catalog IO, and a concurrent-winner
return can exit before the late append. Those received permission decisions vanish.

Keep one event and the existing `BifrostCatalog` owner. A successful create commits
it with the catalog row. A concurrent no-op or other result with a usable operation
transaction commits it there before return. A pre-transaction failure records it
once through the existing standalone server boundary. An audit failure remains
`AuditUnavailable` and permits no unauthorized effect. Preserve built-ins as
unaudited, advisory locking, fingerprint/layout semantics, physical validation,
and idempotent outcomes. Do not add a preliminary unconditional append, second
writer, or transaction abstraction.

### 3. Audit trusted-issuer permission before external IO (`FIND-TASK-001-R1-3`)

Trusted-issuer creation evaluates `service_accounts:write`, then performs
tenant-directed OIDC discovery plus fallible conversion/sealing before persisting
the Allowed event. A discovery or sealing failure loses the verdict, and network
IO starts unaudited.

Complete safe local request validation first. Once permission is evaluated, commit
the Allowed event through the existing standalone audit boundary before discovery.
Then perform the already screened/pinned OIDC request and secret handling, and
insert the issuer without appending again. Never hold a tenant transaction across
network IO. Preserve SSRF controls, secret sealing, response/error behavior, and
denial anti-enumeration. Workload binding needs no change; its proposed failure is
not reachable with the current closed `CardRef` shape.

### 4. Couple the three transactional Card write boundaries (`FIND-TASK-001-R1-4`)

Card registration and delete-by-UID/delete-by-ref currently commit Allowed audit
before entering their authoritative SQL transaction. Later SQL failure can leave a
decision record without the paired effect.

Reuse `authorize_recording_denial`, `append_on`, and the existing Card service
transactions. Pass the Allowed event into registration's authoritative write and
both soft-delete transactions, append before mutation, and commit together. A
no-write validation, replay, conflict, or not-found result still records exactly
one received decision through the shortest existing valid boundary. Preserve one
decision per idempotent request, post-commit storage cleanup, and failure mapping.
Do not change Card reads or completion, and do not move audit into `wyrd-storage`:
those workflows have external/multi-transaction sagas with no safe encompassing
transaction.

### 5. Prove the production concurrency ceiling (`FIND-TASK-001-R1-5`)

The current unit test proves only that `futures-util` obeys the limit it is handed;
it never invokes `AuditPublisher` and cannot regress when production becomes
serial, unbounded, or differently bounded.

Delete that test and its `InFlight` scaffolding. Extend the existing real-server
audit-publication journey, using its existing tenant rows and locks, so more than
the fixed eight tenant cycles enter the production background sweep. Demonstrate
that at most eight reach the blocked publication point, an additional tenant waits,
and all tenants progress and drain after release. Do not add a seam, harness, file,
configuration knob, scheduler, lease, or dependency.

### 6. Restore exact executable proof (`FIND-TASK-001-R1-6`)

The changed reconciliation dead-letter assertion has no green run because the
existing Local-storage test server starts an incompatible Forge worker. INV-025
and TASK-001 do not permit a baseline waiver for a test R1 materially changed.

Correct the existing Local/Forge test composition at its shared test owner so the
changed `card_reconciler_dead_letters_after_three_failures` scenario runs without
weakening Forge production readiness or the test, then record its exact focused
command. Do not repair or require the full Card, CLI, or WyrdState lanes here:
TASK-002 owns their stale Postgres launchers and remaining failures.

### 7. Finish the touched Rust and documentation contract (`FIND-TASK-001-R1-7`)

The prior correction was partial. Touched signatures still use qualified type
paths; changed functions/tests lack mandatory rustdoc sections; and Card service,
test, and Wyrd-design text still describes lifecycle/dead-letter audit events that
no longer exist.

Perform one cumulative diff-scoped inventory. Import existing owning types at the
module top and use bare names in every touched field, alias, parameter, return, and
bound. Add accurate intent and required `# Errors`, `# Panics`, cancellation, or
partial-progress documentation to touched items only. Align Card deletion,
activation, abort, blob-failure, and reconciliation prose with authorization-only
audit and operational lineage; rename changed tests whose names still claim audit.
Regenerate public docs only through their owner. Do not begin a crate-wide cleanup,
add aliases/wrappers, or edit generated outputs by hand.

### 8. Correct R1 lifecycle metadata (`FIND-TASK-001-R1-8`)

Set TASK-001-R1 to `status: review`. Leave TASK-001, TASK-005, and TASK-006 in
`review`. Set TASK-001-R2 to `review` only after its implementation evidence is
complete; `approved` remains the later PASS transition.

### 9. Use the configured authorization owner (`FIND-TASK-001-R1-9`)

The shared service-account audit helper bypasses `PermissionCheck` and directly
inspects permissions through a shortcut helper. A configured/injected checker can
therefore disagree with the verdict that admin, issuance, and revocation audit and
execute.

Inside the existing helper, evaluate `Permission::service_accounts_write()` through
`state.authz.permission_check`, audit that exact Allowed or Denied verdict once, and
map denial through the established public error while preserving the existing
action-specific message/details. Keep `action` only for wire-compatible denial
text. Add no checker, trait, adapter, or second helper.

## Constraints and preserved behavior

- Preserve Redux as the sole Bifrost engine, tenant-qualified identity, RLS, the
  gapless audit chain, Oracle WAL-first acceptance, Scribe WAL/batch fences, Forge
  lineage/readiness, and frozen-range publication.
- Preserve one local Scribe publication path, short tenant transactions, and no
  transaction spanning Scribe, object-store, filesystem, or OIDC network IO.
- Preserve denial semantics, request-level audit cardinality, idempotent replays,
  existing stable error codes, SSRF pinning, and secret redaction.
- Add no compatibility surface, alternate audit history, durability identity,
  migration, scheduler, lease, claim table, configuration, dependency, public
  test seam, harness, or test file.
- Do not broaden into SDK convergence, the single-data-root task, UI work,
  TASK-002's Postgres launcher repair, unrelated CLI failures, or general
  documentation cleanup.

## Acceptance criteria

| Criterion | Finding closure |
|---|---|
| A system-owner peer/tail rejection retains exactly once through the production publisher and leaves system staging empty; non-audit nil ingress still fails | R1-1 |
| Every permitted Bifrost registration request, including a pre-append failure and same-FQN race, records exactly one verdict; successful create remains transactionally coupled | R1-2 |
| Trusted-issuer discovery/sealing failure after an Allowed verdict records one event before external IO and creates no issuer; no DB transaction spans discovery | R1-3 |
| Card registration and both deletes commit or roll back their Allowed row with the authoritative SQL effect; no-write outcomes still record one request decision | R1-4 |
| Production audit sweep demonstrates unordered progress and never exceeds the fixed concurrency bound; disconnected combinator scaffolding is absent | R1-5 |
| The R1-edited reconciliation scenario is runnable and has an exact passing command; TASK-002's five owned lanes remain outside R2 | R1-6 |
| Every touched Rust item and contradictory authority statement satisfies the bare-type and rustdoc contracts without unrelated cleanup | R1-7 |
| R1 and R2 use valid lifecycle states for review | R1-8 |
| Admin/issuance/revocation response, effect, and one audit row follow the configured `PermissionCheck` verdict | R1-9 |

## Focused proof

Use existing test targets and fixtures. Confirm all exact names with
`mise exec -- cargo nextest list`, then record exact package, target, features,
profile, ignored-test mode, and `test(=...)` expressions. At minimum, direct proof
must cover:

1. system-owner unverified security-event retained publication and non-audit nil rejection;
2. Bifrost pre-append failure and concurrent same-FQN registration;
3. trusted-issuer discovery/sealing failure after Allowed;
4. Card registration and both delete rollback/no-write boundaries;
5. configured-checker disagreement at one service-account route;
6. production sweep progress and fixed ceiling;
7. the repaired reconciliation dead-letter scenario.

Then run the original narrow cumulative verification set:

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run test:sql
mise run test:bifrost:integration:redux
mise run test:bifrost:integration:server
mise run test:bifrost:journey:server
mise run test:bifrost:journey:scribe
mise run test:bifrost:journey:oracle
mise run test:bifrost:journey:forge
mise run test:bifrost:journey:otlp
mise run test:bifrost:journey:mcp
mise run check:tenant-isolation
mise run check:from-pools-allowlist
mise run check:object-store-pin
mise run check:unwrap-audit
mise run check:client-tier
mise run check:pyo3-scope
mise run docs:check
git diff --check
```

Do not add `verify:bifrost`; TASK-001 expressly defers that aggregate to the
integration task that owns merge closeout.
