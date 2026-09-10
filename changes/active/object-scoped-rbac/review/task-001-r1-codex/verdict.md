---
task: TASK-001
review: task-001-r1-codex
verdict: FIX_REQUIRED
base: 1c4b518dbb69dd10b9107c06e8ccb410c67c5449
candidate: 1e5ed56664ed87ba6ac7a30587138abdf137904c
prior_candidate: 3de6aed2f86b4bb039e85c7f62c0a48e12602282
spec: changes/active/object-scoped-rbac/spec.md
original_task: changes/active/object-scoped-rbac/tasks/TASK-001-object-scoped-bifrost-rbac.md
prior_verdict: changes/active/object-scoped-rbac/review/task-001-codex/verdict.md
remediation_task: changes/active/object-scoped-rbac/review/task-001-codex/TASK-001-R1-close-rbac-contract-gaps.md
---

# TASK-001 cumulative re-review

## Immutable subject

- Repository root: `/Users/stevenforrester/Documents/GitHub/rbac-alignment`
- Base: `1c4b518dbb69dd10b9107c06e8ccb410c67c5449`
- Cumulative candidate: `1e5ed56664ed87ba6ac7a30587138abdf137904c`
- Approved specification: `changes/active/object-scoped-rbac/spec.md`, revision 3
- Original task: `changes/active/object-scoped-rbac/tasks/TASK-001-object-scoped-bifrost-rbac.md`
- Prior verdict and findings: `changes/active/object-scoped-rbac/review/task-001-codex/verdict.md`
- Remediation task and recorded evidence: `changes/active/object-scoped-rbac/review/task-001-codex/TASK-001-R1-close-rbac-contract-gaps.md`

The candidate remained at the stated commit throughout review. The cumulative
base-to-candidate diff and the remediation diff were inspected. A separate
security audit independently confirmed the remaining authorization-order issue.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001: required typed scope, exact JSON, validation, and three-axis subsumption | `wyrd-spec::auth::permission_scope`; `wyrd-runtime::Permission::{validate,covers}` | Exact `wyrd-spec` 6-test and `wyrd-runtime` 8-test expressions rerun: 14/14 pass | PASS |
| REQ-001 / INV-004: Bifrost scope is valid only on approved exact-read permissions | `Permission::validate` rejects every non-`Read` action and incompatible resource | Exact runtime test passes; recorded exact resolver test covers persisted role JSON | PASS |
| REQ-001 / AC-002 / INV-005: scoped role resolution retains the existing tenant, cache, and epoch path | Existing `SqlPermissionResolver` decodes `Vec<Permission>` into `PermissionSet`; no alternate store or checker | Recorded exact resolver and `wyrd-auth-verify` expressions pass | PASS |
| REQ-002 / INV-001: resolved identity and tenancy come from the verified principal and tenant-bound catalog | `attempt_protected_cut` prepares `TenantTableBinding` and stable `TableUid` from `context.data_tenant_id`; scope is projected from that identity | Exact reader-authority test rerun and passes | PASS |
| REQ-002 / INV-003: SQL queries authorize the complete prepared table set before reader guard, revalidation, materialization, source IO, accepted audit, peer dispatch, and admission | `OraclePlanner::attempt_protected_cut` authorizes at `planner.rs:213` before guard/materialization; SQL planning precedes admission | Exact reader-authority test rerun: denial leaves sealed-pin count at zero | PASS |
| REQ-002: typed/analytical plans converge on the same decision before admission | `query_plan` acquires `AdmittedQueryGuard` at `oracle/mod.rs:3436-3445`; object authorization occurs later through `execute_typed_plan` -> `prepare_typed_cuts` at `3525-3538` | Existing direct planner test cannot observe typed-plan admission ordering | FAIL (`FIND-TASK-001-1`) |
| REQ-002 / AC-005: payload authority remains separately object-scoped and distributed authority cannot widen | resolved table scope feeds payload permissions and the scoped permission digest | Recorded exact payload/digest and peer-authority expressions pass | PASS |
| REQ-003 / AC-003 / AC-004: tenant-local analyst/data-scientist matrix accepts and rejects the required queries with no denied stream | Real server journey seeds only tenant-local roles and drives the public SDK | Exact journey rerun: 1/1 passes, including all six matrix cases | PASS |
| INV-005: public scoped object denials are durable, emit no accepted-read event, and fail closed when append fails | `QueryAuthority::record_object_denial` reuses the tenant outbox; `stream_query` handles Oracle `QueryForbidden` | Exact real-server journey rerun and passes | PASS |
| Repository struct-centered rule | `QueryAuthority` cohesively owns route admission, denial audit, and lifecycle authorization/audit | Source inspection and recorded lints | PASS |
| Repository test documentation rule / prior FIND-TASK-001-6 | Most cumulative new tests have intent/invariant rustdoc, but `permission::tests::malformed_scope_identity_is_rejected_at_decode` does not; its intended comment is attached to the preceding action test | Cumulative diff and source inspection at `permission.rs:968-1009` | FAIL (`FIND-TASK-001-6`) |
| Prior FIND-TASK-001-4: active foundation docs state literal scope/wildcard rules | Narrow-wildcard claim removed; delegation examples use `All` | Source inspection and recorded `docs:check` | PASS |
| Prior FIND-TASK-001-7 / AC-006: exact focused proof and affected checks are recorded | R1 report records fully qualified exact nextest expressions and run counts | Selected exact tests rerun; recorded fmt, lints, codegen, boundary, docs, and journey evidence is credible | PASS |
| Constraints and non-goals | No grant store, migration, policy lookup, checker, scope language, table identity, dependency, builtin role, compatibility path, or public error was added | Cumulative name/status diff and source inspection | PASS |

## Prior-finding closure

| Finding | Closure |
|---|---|
| `FIND-TASK-001-1` | PARTIAL — SQL and shared cut materialization now authorize before source IO, but the typed-plan path still acquires admission first. |
| `FIND-TASK-001-2` | CLOSED — public scoped denials use the existing durable tenant outbox and fail closed. |
| `FIND-TASK-001-3` | CLOSED — Bifrost scope requires exact `Read` at the shared decoder. |
| `FIND-TASK-001-4` | CLOSED — the active permission model is literal about wildcard scope and required `All`. |
| `FIND-TASK-001-5` | CLOSED — `QueryAuthority` owns the related stateful workflows. |
| `FIND-TASK-001-6` | OPEN — one cumulative new test remains undocumented. |
| `FIND-TASK-001-7` | CLOSED — exact selectors and run counts are recorded. |

## Material findings

### FIND-TASK-001-1 — INCORRECT: typed-plan object authorization follows admission

- Violated obligation: REQ-002 requires every interactive, analytical, and other query path to take its final resolved-object decision before admission.
- Location: `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3436-3446` and `:3525-3538`; the decision itself is `crates/vala/vala-bifrost-redux/src/oracle/planner.rs:207-217`.
- Evidence: `Oracle::query_plan` acquires an `AdmittedQueryGuard` before calling `execute_typed_plan`; only inside that call does `prepare_typed_cuts` reach `authorize_resolved_tables`.
- Observable consequence: an out-of-scope typed-plan caller can enter or wait in tenant/global Oracle admission and hold admitted capacity until catalog preparation rejects the plan. Data remains fail-closed, but authorization does not precede the explicitly protected capacity boundary.
- Required correction: prepare and authorize the complete typed cut before acquiring admission, carry that already-authorized cut into execution without repinning, and prove an uncovered typed plan produces no admission observation.

### FIND-TASK-001-6 — VIOLATION: one cumulative new test lacks rustdoc

- Violated obligation: `architecture/agent-rules.md` and the R1 acceptance criterion require intent/invariant rustdoc on every new test.
- Location: `crates/shared/wyrd-runtime/src/permission.rs:968-1009`.
- Evidence: the comment beginning "Proves a structurally valid but empty object identity" precedes `bifrost_scope_is_rejected_on_a_non_read_action`; `malformed_scope_identity_is_rejected_at_decode` begins with `#[test]` and no rustdoc.
- Observable consequence: the malformed-identity regression test does not state its protected contract, while the preceding test carries a misleading mixed-purpose comment.
- Required correction: move or add the malformed-identity intent comment immediately above its test and leave the action test with only its own intent.

## Verification limits

- The accepted journey cases still stream zero rows; they prove successful authorization, admission, audit, and execution completion rather than projection contents. Denials are pre-stream and return no rows.
- `mise run gate` and `mise run test:bifrost` were not run, as prohibited by the task.
- The reported `check:design-sync` and `check:unwrap-audit` failures are unchanged at the base and outside this write set; they are not findings.

## Verdict

`FIX_REQUIRED`
