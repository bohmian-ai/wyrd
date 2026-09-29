# TASK-003 Review Verdict

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Candidate: `467ea07d94a5a6d665d24f56afc0bc0a12532304`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- Review attempt: `TASK-003-r1`

The candidate remained at the pinned commit throughout both review waves. The
review artifacts are not part of the reviewed candidate.

## Acceptance Matrix

| Obligation | Result | Evidence or retained finding |
|---|---|---|
| Registration atomically persists the Card, Card-bound principal, binding projection, and initial cursor state through the caller-owned `TenantConn` | PASS | Registration, SQL projection, forced-RLS migration, rollback tests |
| Stable typed UUIDv7 binding identity follows the approved natural key | FAIL | Owner occurrences use nullable `None` instead of the reserved non-null owner value (`FIND-TASK-003-5`) |
| Referenced Trigger/Operator identities and inline digests are frozen in the binding projection | PASS | Projection source and registration-route assertions |
| Invalid binding input is refused before durable writes | FAIL | A syntactically valid schedule with no possible future occurrence is accepted and can never arm (`FIND-TASK-003-4`) |
| Only a successful API-key or workload `jwt-bearer` exchange activates the exact Card-bound owner | FAIL | Shared partial-CardRef lookup can select a sibling-space principal (`FIND-TASK-003-2`) |
| Successful exchanges monotonically renew shared principal activity without moving an armed cursor | FAIL | An older concurrent exchange timestamp can overwrite a newer one (`FIND-TASK-003-3`) |
| Current principal/Card state and the inactivity timeout gate new binding work | PASS | Tenant-scoped activity query and lifecycle/timeout tests |
| Operator-bearing registration requires and audits `operators:invoke` in addition to `cards:write` | FAIL | Registration evaluates only `cards:write` (`FIND-TASK-003-1`) |
| Card GET exposes server-derived stable binding IDs with normal authorization and tenant isolation | PASS implementation / FAIL proof | Raw HTTP proof exists; first-class SDK journeys do not prove the public status field (`FIND-TASK-003-9`) |
| Runtime OpenAPI exposes the new verification status shape | FAIL proof | Served-document test does not assert `verification.binding_ids` (`FIND-TASK-003-10`) |
| Existing qualifying, excluded, expiry, lifecycle, A/B, and replica activity paths have the required real-client/server evidence | FAIL proof | Required AC-019 journey coverage is incomplete (`FIND-TASK-003-8`) |
| Durable identity APIs use domain types | FAIL | Activity SQL API leaks raw UUIDs (`FIND-TASK-003-6`) |
| New dependency-backed Rust workflows have a cohesive concrete owner | FAIL | Binding freeze/projection remains a dependency-threaded free workflow (`FIND-TASK-003-7`) |
| New and materially modified Rust items meet repository rustdoc requirements | FAIL | SQL constants, associated items, and test helpers/tests are incompletely documented (`FIND-TASK-003-11`) |
| No heartbeat, activity table, activation endpoint, idle refresh, per-request touch, Vala control store, or second binding identity was introduced | PASS | Complete diff inspection |

## Review Results

| Review | Result | Report |
|---|---|---|
| Task implementation | FAIL | `task-review.md` |
| Repository standards | FAIL | `standards-review.md` |
| Contracts domain | FAIL | `domain-review-contracts.md` |
| Security and tenancy domain | FAIL | `domain-review-security-tenancy.md` |
| Data and durability domain | FAIL | `domain-review-data-durability.md` |
| Structured Ponytail validation | COMPLETE | `findings-validation.md` |

## Validated Finding Ledger

| Finding | Status | Required outcome |
|---|---|---|
| `FIND-TASK-003-1` | CONFIRMED | Enforce and transactionally audit `operators:invoke` for Operator-bearing registration. |
| `FIND-TASK-003-2` | REVISED | Make the shared CardRef-to-principal lookup fail closed on ambiguity while preserving explicit exact lookup and A/B versions. |
| `FIND-TASK-003-3` | CONFIRMED | Make `last_authenticated_at` monotonic in Postgres. |
| `FIND-TASK-003-4` | CONFIRMED | Reject schedules that have no future occurrence before registration writes. |
| `FIND-TASK-003-5` | CONFIRMED | Persist the approved non-null reserved owner occurrence key. |
| `FIND-TASK-003-6` | CONFIRMED | Use existing typed identities at verification SQL API boundaries. |
| `FIND-TASK-003-7` | REVISED | Give only the dependency-backed binding freeze/projection workflow a concrete transaction-scoped owner. |
| `FIND-TASK-003-8` | REVISED | Complete AC-019 real-client/server activity coverage for existing paths; do not fabricate SYSTEM issuance. |
| `FIND-TASK-003-9` | REVISED | Prove binding status through existing Rust, Python, and TypeScript Card journeys. |
| `FIND-TASK-003-10` | CONFIRMED | Assert the status field in the served OpenAPI contract. |
| `FIND-TASK-003-11` | REVISED | Complete mandatory rustdoc across all added/materially modified Rust items. |

The detailed source locations, evidence, consequences, correction boundaries,
and focused closure proofs are authoritative in `findings-validation.md`.

## Verification Limits

- The implementation record reports all listed narrow lanes passing except
  `mise run test:e2e`, which is not defined; it records three existing journey
  lanes as substitutes.
- Wave 1 independently reran tenant-isolation, registry-transaction coupling,
  whitespace, and selected focused SQL tests. The other recorded broad lanes
  were inspected as evidence but were not all rerun during this read-only
  review.
- Green aggregate lanes cannot close the retained untested paths. Remediation
  must run the focused proofs and the owning broader lanes named in
  `TASK-003-R1-close-binding-activity-contract.md`.
- The SYSTEM token path remains outside TASK-003 because it does not exist
  until TASK-004. No retained finding requires it.

## Prior-Finding Closure

This is the first TASK-003 review. There are no prior `FIND-TASK-003-*`
findings to reassess. A remediation review must preserve these IDs and review
the complete original base through the new cumulative candidate.

## Verdict

**FIX_REQUIRED**

All retained findings are bounded implementation or proof gaps within approved
specification revision 33. None requires a new product, public API,
architecture, security, concurrency, resource-ownership, or persistent-data
decision.
