# TASK-001-R1 independent Ponytail finding validation

## Subject and method

- Base: `8377fff9f03cc60de4be3e088569e38984382dc4`
- Candidate: `8e61c03493f5c0ebcc12cdb4a9998df284f7fc8f`
- Authority: approved specification revision 7, original TASK-001, prior verdicts,
  TASK-001-R1, repository rules, complete remediation diff, and current callers
- Validator: fresh independent Ponytail reviewer; no implementation or artifact edits

Every proposed finding was traced through its callers and complete correction
boundary. The validator rejected speculative paths, reused existing owners, and
required no new abstraction, dependency, configuration, public seam, or harness.

## Dispositions and decision-complete recommendations

| Finding | Disposition | Minimum decision-complete recommendation |
|---|---|---|
| `FIND-TASK-001-R1-1` | CONFIRMED | Permit `DataTenantId::SYSTEM_OWNER` only through the internal canonical `vala.system.audit_log` publication path at each current nil guard. Retain nil rejection for caller-owned tables and non-audit ingress. Reuse the existing system tenant, Audit namespace/table, publisher, Scribe fence, and server journey infrastructure. |
| `FIND-TASK-001-R1-2` | CONFIRMED | Keep one handed-back Allowed event across `register_table` and the existing catalog registration transaction. Every result must either consume it transactionally with create/concurrent no-op, record it standalone exactly once when no usable operation transaction exists, or fail closed as audit unavailable. Built-ins remain unaudited. |
| `FIND-TASK-001-R1-3` | REVISED | Correct trusted-issuer create only. Persist Allowed through the existing standalone audit boundary before OIDC network IO, then perform discovery/sealing and open the insert transaction without a second append. Do not hold a DB transaction across discovery. The workload-binding serialization subclaim is rejected because its closed `CardRef` shape has no reachable serialization failure. |
| `FIND-TASK-001-R1-4` | REVISED | Couple only Card registration and delete-by-UID/delete-by-ref to their existing authoritative SQL transactions. Keep Card reads/completion and all storage sagas standalone because no single transaction can encompass their external IO safely. |
| `FIND-TASK-001-R1-5` | CONFIRMED | Delete the integer-only `for_each_concurrent` demonstration. Exercise the production-selected bounded dispatch through the existing real-server publication journey with more tenants than the fixed bound and existing locks; prove both progress and the ceiling without a new seam or harness. |
| `FIND-TASK-001-R1-6` | REVISED | Make only the R1-edited reconciliation test executable and record its exact focused command. Transfer repair of the five stale Card/CLI/WyrdState `mise` launchers to TASK-002, which owns those lanes; do not require their full suites for TASK-001-R2. |
| `FIND-TASK-001-R1-7` | REVISED | Perform one diff-scoped correction: bare imports/signatures; accurate Card permission-versus-lineage prose; and required rustdoc/`# Errors`/`# Panics` on touched items only. No broad cleanup. |
| `FIND-TASK-001-R1-8` | CONFIRMED | Change R1 from `implemented` to `review`; no new lifecycle state. |
| `FIND-TASK-001-R1-9` | CONFIRMED | Route `service_accounts:write` through `state.authz.permission_check`, audit that exact verdict, and preserve the existing action-specific denial response. Add no checker or trait. |

## Validation detail

### FIND-TASK-001-R1-1 — System-owner publication

The tenant directory deliberately maps the nil active row to `SYSTEM_OWNER`.
The publisher visits it, but audit projection, generic Scribe ingress, and
physical tenant binding reject nil. Unverified peer and tail rejections really
append under this tenant. REQ-026B fixes that attribution; REQ-027 requires its
publication; Bifrost design fixes the sentinel itself to nil. Those authorities
already choose the outcome. The valid minimum is an audit-only internal exception,
not dropping the tenant, remapping its hash identity, or globally allowing nil.

Direct proof must append an unverified peer or tail denial, retain it exactly once
through the production publisher/Scribe path, drain system staging, and preserve
a negative non-audit nil-ingress test.

### FIND-TASK-001-R1-2 — Bifrost registration lost verdicts

`register_table` evaluates permission before its observed-not-found branch, but
`create_table_locked` can fail throughout validation, catalog, physical IO, or a
concurrent-winner return before its late audit append. The correction stays in
the existing server/catalog owners and must preserve advisory locking, conflicts,
physical validation, and same-transaction successful creation. Proof covers one
pre-append failure and one same-FQN race, with one Allowed row per request.

### FIND-TASK-001-R1-3 — Trusted issuer external IO before audit

OIDC discovery and secret conversion/sealing occur after the permission verdict
but before append. Reachable discovery or sealing failure loses the decision and
allows unaudited tenant-directed network IO. Local validation may precede the
verdict; once evaluated, the Allowed row must commit before discovery. Proof uses
existing mock discovery and failure seams and asserts one decision, no issuer row,
and unchanged SSRF/secret behavior.

### FIND-TASK-001-R1-4 — Card transaction coupling

Card registration and both delete routes have one authoritative SQL mutation
transaction yet use standalone `audit::authorize`. Their service owners must
receive the Allowed event and append before mutation. No-write validation,
replay, or not-found outcomes still record one request decision. Completion and
storage remain standalone controls because forcing their sagas into one DB
transaction would violate short-transaction and external-IO rules.

### FIND-TASK-001-R1-5 — Disconnected concurrency proof

The test invokes `futures-util` directly over integers and never calls the
publisher. It remains green if production becomes serial, unbounded, or uses a
different limit. The existing journey proves only non-serial progress. Directly
exercise the production sweep and delete the duplicate scaffolding.

### FIND-TASK-001-R1-6 — Missing exact proof

The candidate itself records the changed reconciliation test as unexecuted.
INV-025 and R1 provide no baseline waiver for a test R1 materially edited.
Repair only the existing Local/Forge composition needed to run
`card_reconciler_dead_letters_after_three_failures`, then record its exact
focused command. TASK-001's declared Bifrost-focused lanes and scenarios pass;
the five stale Card/CLI/WyrdState launchers and their full suites belong to
TASK-002.

### FIND-TASK-001-R1-7 — Rust source and documentation standards

Confirmed groups are: fully qualified types in touched signatures; stale Card
service, test, and Wyrd-design prose that still describes removed lifecycle
audit; and missing rustdoc/error/panic contracts on touched functions/tests.
Only changed or directly contradictory items belong in the correction.

### FIND-TASK-001-R1-8 — Lifecycle

`implemented` is not valid task metadata. The one-line `review` correction is
the entire boundary.

### FIND-TASK-001-R1-9 — Authorization chokepoint

The new service-account audit helper directly inspects effective permissions
through a shortcut helper rather than the configured `PermissionCheck`. One
focused injected-checker scenario must prove response, effect, and exactly one
audit row follow the configured verdict.

## Rejected or narrowed claims

- No specification revision is required for system audit: approved authority
  already fixes the nil system tenant and requires its retained publication.
- Workload-binding JSON failure is unreachable for the closed current `CardRef`.
- Card reads/completion and storage sagas must not be forced into long database
  transactions.
- Unrelated baseline CLI failures are not candidate defects and are excluded.
- The missing `setup:db-roles` launcher defect transfers to TASK-002 for all
  five lanes it explicitly owns; it is not TASK-001-R2 remediation.

## Scope revalidation

A fresh independent Ponytail reviewer revalidated finding 6 after the merge-task
boundary was clarified. It confirmed that TASK-002 explicitly owns the five
affected lanes and that each still depends on `setup:postgres`, whose
`setup:db-roles` dependency no longer exists; it also confirmed that R1 itself
materially changed the dead-letter test, so only that exact proof remains in R2.
