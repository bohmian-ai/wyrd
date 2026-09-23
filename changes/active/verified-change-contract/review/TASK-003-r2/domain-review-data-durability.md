# TASK-003 R2 Data and Durability Domain Review

## Reviewed Boundary

- Immutable base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Cumulative candidate: `449aceb346f5f5fbc27958d260bd9c0c50225466`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- Prior review inputs: `review/TASK-003-r1/verdict.md`, `findings-validation.md`,
  `domain-review-data-durability.md`, and
  `TASK-003-R1-close-binding-activity-contract.md`
- Domain scope: migration safety, persistent binding identity and frozen data,
  principal lookup after the A/B uniqueness change, registration and issuance
  transaction composition, forced RLS, schedule-cursor durability, activity
  monotonicity, lifecycle admission reads, and the persistence-level proof of
  those properties.

The review covered the complete base-to-candidate diff. It traced the current
registration path from `write_registration` through `persist_node`,
`BindingProjector`, and `project_bindings`; the shared credential lookup through
API-key issuance, workload `jwt-bearer`, and delegation; and the token-issuance
path through `TenantTokenIssuer::issue` and
`record_machine_authentication`. `.codegraph/` is absent, so source and caller
coverage used repository search and direct file inspection.

## Authority and Source Coverage

| Authority or source | Coverage |
|---|---|
| `AGENTS.md` sections 2-6 and 9-12 | Durable server ownership, typed identities, `TenantConn` composition, forced RLS, struct-centered Rust, test tiers, and completion rules. |
| `architecture/agent-rules.md` | Caller-owned transaction lifecycle, RLS as the tenant boundary, SQL connection types, typed identity, and exact verification requirements. |
| `architecture/wyrd-design.md` and `architecture/wyrd-doctrine.mdx` | Exact Card-version identity, server-owned verification behavior, and binding direction. |
| `architecture/wyrd-security-posture.md` | Exact Card-bound principals, fail-closed ambiguity, token issuance, current lifecycle state, and tenant isolation. |
| `architecture/v1/00-foundations/sql-foundation.md` | Tenant keys, composite tenant foreign keys, transaction ownership, migration immutability, and SQL verification. |
| `architecture/references/architecture/patterns.md` | Registry/storage ownership and tenant-scoped SQL composition. |
| `architecture/references/languages/rust-core.md` | Domain identities and dependency-owning workflow structure. |
| `architecture/references/languages/testing-workflows.md` and `spec-driven-development.md` | Postgres integration proof, exact test selection, cumulative remediation review, and evidence authority. |
| Specification REQ-078, REQ-104-108, REQ-112, REQ-134; AC-018-020, AC-028 | Binding storage, stable natural-key identity, exact qualifying activity, monotonic renewal, no backfill, transaction coupling, status, and required proof. |
| Migration and production source | Full migration 27; `queries/verification.rs`; shared CardRef lookup; registration resolution/projection; token issuance; binding-status hydration; all production callers of the corrected lookup and activity writer. |
| Tests and remediation evidence | Full `pg_verification_bindings.rs`; relevant registration-route, identity, SDK, and OpenAPI additions; R1 remediation evidence treated as evidence rather than authority. |

## Prior-Finding Closure

| Prior finding | R2 assessment | Current evidence |
|---|---|---|
| `FIND-TASK-003-2` | **CLOSED** | `service_account_by_card_ref` now fetches at most two active containment matches and returns a principal only for exactly one match. API-key issuance, workload `jwt-bearer`, and CardRef-targeted delegation still share this owner. The old creation-order selector is gone. Same-space exact paths and multi-space refusal are covered at the auth/server seams. |
| `FIND-TASK-003-3` | **CLOSED** | `RECORD_AUTHENTICATION_SQL` uses `GREATEST(last_authenticated_at, $2)`, so a later-committing older exchange cannot move durable activity backward. The principal update serializes concurrent issuers before null-cursor arming; the cursor update remains null-only. `out_of_order_exchanges_never_move_activity_backward` proves the stored timestamp, eligibility window, and cursor stability. |
| `FIND-TASK-003-4` | **CLOSED** | Pre-write effective-binding validation now calls `BindingSchedule::next_after(Utc::now())` after parsing. A syntactically valid but unreachable schedule is rejected before the registration transaction, while the issuance path retains defensive handling for persisted invalid data. |
| `FIND-TASK-003-5` | **CLOSED** | Migration 27 makes `subject_occurrence_key` non-null and keys owner occurrences with the shared `$owner` constant. Service composition rejects `$owner` as a component alias; the table additionally restricts Agent bindings to `$owner`. Natural-key UUIDv7 reuse and component aliases remain unchanged. |
| `FIND-TASK-003-6` | **CLOSED** | Public verification query boundaries now accept `PrincipalId` and return `BindingId`, `CardUid`, and `PrincipalId`. Raw SQL UUIDs remain private decode state, and stored binding/owner UUID versions are checked before return. |
| `FIND-TASK-003-7` | **CLOSED** | `BindingProjector` owns the borrowed registration `TenantConn` and the multi-step freeze/project workflow. `persist_node` invokes it only after the Card and Card-bound principal writes in the same caller-owned transaction. Pure identity/digest helpers and narrow SQL operations remain direct. |

## Persistent-Data Seam Assessment

| Seam | Result | Evidence |
|---|---|---|
| Registration atomicity | PASS | `write_registration` owns one `TenantConn` and commits only after every topo-ordered Card, principal, relationship, binding, operation, and audit write. `BindingProjector` and `project_bindings` never commit. Rollback and cross-tenant projection cases exercise the seam. |
| Stable binding identity | PASS | The table's unique natural key is exactly `(data_tenant_id, owner_card_uid, subject_occurrence_key, verifier_uid)`. Conflict returns the stored identity, owner/component occurrence domains are disjoint, and typed decode rejects non-v7 binding IDs. |
| Frozen binding data | PASS | Projected rows persist exact owner/subject/Verifier UIDs, referenced Trigger/Operator UIDs or inline canonical digests, activation kind, and schedule. The production projector derives these only after reference resolution and pre-write validation. |
| Tenant isolation | PASS | The table carries `data_tenant_id`, has enabled and forced RLS with the canonical policy, grants only required tenant operations, and all runtime queries accept `TenantConn`. The owner foreign key is tenant-qualified; no raw pool or callee transaction control entered the path. |
| Principal A/B compatibility | PASS | Migration 27 removes only the tenant-wide name uniqueness that prevented two exact Card versions, retains the exact Card-binding unique key, and preserves name uniqueness for Card-free principals. Shared partial CardRef lookup now refuses multiple active matches instead of choosing by age. |
| Activity durability and cursor arming | PASS | Qualifying issuance updates activity in the caller's issuance transaction using server time. The timestamp is monotonic, schedule rows are locked in binding-ID order, and cursor writes are null-only. Nonqualifying grants never call the writer; inactive or Card-free principals are no-ops. |
| Admission read | PASS | `binding_activity` derives eligibility from the exact owner Card and Card-bound principal's current statuses plus the persisted timestamp and supplied timeout. Component bindings inherit the owner principal; A/B owner UIDs remain independent. |
| Migration compatibility | PASS | The migration is additive except for the deliberate uniqueness relaxation required by REQ-112. No pre-change Card can contain the newly introduced binding contract from TASK-001, so no binding-row backfill is required by this task. Existing principal rows retain null activity until a qualifying exchange. |

## Material Findings

None. No reachable data-loss, cross-tenant, identity-selection, transaction,
concurrency, or persistent-state defect remains within the approved TASK-003
boundary. Optional database hardening beyond the server-owned typed write path
was not elevated into a finding.

## Verification Limits

- The review independently inspected every relevant production caller and the
  complete persistence test bodies but did not rerun Cargo or Postgres lanes in
  the shared checkout. The remediation record reports the focused SQL tests,
  `test:sql`, registration/auth journeys, SDK journeys, RLS checks, format, and
  lints passing; those results are supporting evidence, not authority.
- `git diff --check base..candidate` was rerun and currently fails only on
  trailing spaces in the preserved R1 `standards-review.md` artifact. That is
  outside this data/durability boundary and does not falsify the closure above,
  but the final task orchestrator must account for the repository completion
  gate.
- Workload configuration requires an explicit space, so an ambiguous
  omitted-space workload assertion is not a reachable public path. Its exact
  path and the shared lookup used by all credential/delegation callers were
  still traced.

## Overall Result

**PASS**

All six specifically assigned prior findings are closed, and the cumulative
candidate satisfies TASK-003's persistent-data, transaction, tenancy,
durability, and concurrency obligations without a new domain finding.
