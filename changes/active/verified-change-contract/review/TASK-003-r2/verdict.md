# TASK-003 R2 Review Verdict

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Original base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Cumulative candidate: `449aceb346f5f5fbc27958d260bd9c0c50225466`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- Prior verdict and findings: `changes/active/verified-change-contract/review/TASK-003-r1/`
- Prior remediation: `changes/active/verified-change-contract/review/TASK-003-r1/TASK-003-R1-close-binding-activity-contract.md`
- Review attempt: `TASK-003-r2`

The candidate remained pinned throughout both review waves. The uncommitted R2
review artifacts are not part of the reviewed candidate.

## Acceptance Matrix

| Obligation | Result | Evidence or retained finding |
|---|---|---|
| Card, principal, binding projection, and audit writes remain transactionally composed through caller-owned `TenantConn` | PASS | Cumulative source and Postgres registration tests |
| Binding IDs use stable UUIDv7 identity with the approved total owner/component occurrence key | PASS | Non-null `$owner`, alias exclusion, typed decoding, reapply/reorder tests |
| Trigger/Operator identities and inline digests are frozen exactly | PASS | `BindingProjector` and route/SQL freeze coverage |
| Operator-bearing registration enforces and audits `operators:invoke` | PASS | Conditional authorization, rollback, and audit-cardinality journey |
| CardRef principal lookup fails closed on ambiguity across API-key, workload, and delegation callers | PASS behavior / FAIL repository boundary | Exact-one selection is correct, but its `TenantConn` query retains a prohibited manual tenant predicate (`FIND-TASK-003-12`) |
| Qualifying authentication records monotonic exact-owner activity and preserves armed cursors | PASS | Postgres `GREATEST`, reverse-order test, API-key and workload journeys |
| Excluded grants/uses, lifecycle changes, inactivity, A/B versions, replicas, and observations obey the activity contract | PASS | Assembled server, identity, and observation journeys |
| Impossible schedules are refused before writes | PASS | Pre-write `next_after` validation and rollback assertion |
| Card GET returns stable binding IDs through first-class SDKs | PASS for Rust/Python; FAIL for TypeScript typed contract | TypeScript journey uses a local cast because exported `Card` omits `status` (`FIND-TASK-003-9`) |
| Served OpenAPI exposes the nested verification status and UUID-array shape | PASS | Runtime OpenAPI contract test |
| New workflows use cohesive owners and durable identities use domain types | PASS | `BindingProjector`, `PrincipalId`, `BindingId`, and `CardUid` boundaries |
| New/materially modified Rust items satisfy documentation rules | PASS | Cumulative item audit plus format/lint evidence |
| Tenant isolation relies on forced RLS without redundant manual tenant predicates | FAIL | `service_account_by_card_ref` duplicates the tenant authority (`FIND-TASK-003-12`) |
| Required cumulative completion gates are reproducibly green | FAIL | `git diff --check` reports 28 committed trailing-whitespace errors (`FIND-TASK-003-13`) |
| Prohibited heartbeat/activity/lifecycle/storage mechanisms remain absent | PASS | Complete cumulative diff inspection |

## Wave Results

| Review | Result | Report |
|---|---|---|
| Task implementation | FAIL | `task-review.md` |
| Repository standards | FAIL | `standards-review.md` |
| Contracts and SDK surfaces | FAIL | `domain-review-contracts.md` |
| Security and tenancy | PASS | `domain-review-security-tenancy.md` |
| Data and durability | PASS | `domain-review-data-durability.md` |
| Structured Ponytail validation | COMPLETE | `findings-validation.md` |

## Validated Finding Ledger

| Finding | Status | Required outcome |
|---|---|---|
| `FIND-TASK-003-9` | REVISED / REOPENED | Add the existing complete Card status contract to the exported TypeScript `Card` type and prove direct typed access in the existing journey. |
| `FIND-TASK-003-12` | CONFIRMED | Remove the redundant tenant predicate/bind from the `TenantConn` CardRef lookup while preserving exact-one fail-closed selection. |
| `FIND-TASK-003-13` | REVISED | Remove only the committed trailing spaces and rerun the cumulative whitespace gate. |

Detailed evidence, caller tracing, consequences, correction boundaries, and
focused proofs are authoritative in `findings-validation.md`.

## Prior-Finding Closure

`FIND-TASK-003-1` through `FIND-TASK-003-8`, `FIND-TASK-003-10`, and
`FIND-TASK-003-11` are closed by the cumulative candidate. `FIND-TASK-003-9`
is reopened because the TypeScript runtime journey receives the value but
bypasses the missing exported type with a test-local cast. New findings
`FIND-TASK-003-12` and `FIND-TASK-003-13` are assigned in this review.

## Verification Limits

- Reviewers independently reran focused SQL, registration, identity, RLS,
  codegen, and boundary checks in proportion to their domains; the candidate's
  full evidence table supplied the remaining final-lane results.
- The TypeScript runtime assertion passes, but it is not credible typed-contract
  proof until the local cast is removed and `ts:typecheck` checks the exported
  status shape.
- The exact cumulative `git diff --check` command was rerun and fails, so the
  recorded green claim cannot close that gate.
- The nonexistent TASK-004 SYSTEM issuance path remains correctly excluded.

## Verdict

**FIX_REQUIRED**

The three retained corrections are bounded by approved specification revision
33 and existing repository authority. No specification revision is required.
