# TASK-003 Task Implementation Review

## Immutable Subject

- Base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Candidate: `467ea07d94a5a6d665d24f56afc0bc0a12532304`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- Review scope: complete `base..candidate` diff plus the existing callers and contracts directly affected by that diff

## Acceptance Matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK outcome: atomically project exact owner bindings in tenant Postgres | `service.rs:1074-1126` projects on the registration `TenantConn`; migration forces RLS | `projection_is_transactional_and_tenant_isolated`; server registration route coverage | PASS |
| TASK outcome: one stable UUIDv7 binding identity under the exact natural key | `ids.rs` adds `BindingId`; migration `:51-83` adds the unique key | `binding_id_is_uuid7_backed`; `projection_identity_is_stable_under_reapply_and_reorder` | FAIL — owner occurrences are stored as `NULL`, not the required reserved owner value (TASKREV-003) |
| TASK outcome: runtime activity follows the existing exact Card-bound principal lifecycle | `issuance.rs:349-352`; `verification.rs:298-352` | issuer and SQL activity tests | FAIL — the uniqueness change makes pre-issuance CardRef lookup ambiguous across spaces (TASKREV-002) |
| TASK outcome: status exposes stable binding IDs without a binding resource | `service.rs:93-173,268-339`; `VerificationStatus` in `verifier.rs` | `owner_status_serves_stable_binding_ids_and_exchange_activates` | PASS |
| REQ-078: `wyrd-sql` owns the tenant-RLS binding control state used by this task | migration `20260601000027_verification_bindings.sql`; `queries/verification.rs`; all APIs take `TenantConn` | SQL transaction/RLS test and boundary checks recorded in task evidence | PASS |
| REQ-095: each effective owner/component occurrence projects one subscription | `service.rs:1147-1203` walks Service, components, and standalone Agent sites | Service-level/component projection tests | PASS |
| REQ-102: duplicate verifier binding per subject occurrence is refused | existing `spec_binding_errors` remains on registration validation; database natural key is a second fence | inherited contract tests plus route refusal coverage | PASS |
| REQ-104: typed UUIDv7 ID, exact natural key, stable reapply/reorder behavior, frozen Trigger/Operator identity | `BindingId`; projection query; UID/digest freezing | UUID, reorder, and route assertions | FAIL — exact subject-occurrence representation is not implemented (TASKREV-003) |
| REQ-105: registration is inactive; only successful exact-owner API-key/workload exchange activates; excluded grants do not | registration leaves timestamp/cursor null; `TenantGrant::records_owner_activity` admits only API key/JWT bearer | direct issuer exclusions and one real API-key route | FAIL — ambiguous CardRef lookup can select a different space's owner; required cross-boundary exclusions are not journey-proven (TASKREV-002, TASKREV-004) |
| REQ-106: use server time in the issuance transaction; no heartbeat, timer, request touch, or observation write | `issuance.rs:349-352`; `record_machine_authentication` does not commit; no new activity mechanism in diff | issuer/SQL tests | PASS |
| REQ-107: enabled owner plus recent authentication gates activity, default 86400 seconds | `verification.rs:27-28,358-426` and current Card/principal status joins | deterministic cutoff, suspension, deletion, and version tests | PASS |
| REQ-108: admission query restricts new work to current exact-owner activity without cancelling admitted work | `binding_activity` supplies the downstream admission predicate; cursor renewal does not move an armed cursor | SQL activity and cursor tests; scheduler use remains TASK-004 | PASS |
| REQ-112: owner Card, principal, binding projection, and initial null cursor share registration transaction | `service.rs:999-1070,1074-1126`; migration cursor rules | rollback/projection and public route tests | PASS |
| REQ-134: Card GET projects owner binding IDs into server-derived verification status | `service.rs:93-173,268-339` | public Card GET route assertions | PASS |
| REQ-145: registration requires `cards:write`; a binding with `on_failure` additionally requires and audits `operators:invoke` | registration route checks only `Permission::card_write()` at `routes.rs:385-405`; no conditional Operator authorization exists | no cards-write-only/operator-invoke denial case | FAIL — TASKREV-001 |
| INV-001: bindings remain inline Card declarations, not Cards/resources | only projection rows and status IDs are added | schema/codegen and registration tests | PASS |
| INV-007: tenant isolation and transactional authorization audit | binding rows use forced RLS and Card reads retain normal audit | RLS/read denial tests | FAIL — required `operators:invoke` decision and audit are absent (TASKREV-001) |
| INV-010: Postgres owns operational binding state; no analytical state moved out of Bifrost | binding projection/activity only | diff inspection | PASS |
| AC-018 binding slice: composite registration resolves/freezes bindings, rejects invalid refs/actions, and registration alone stays inactive | `resolve.rs`, `owner_bindings`, registration transaction | real server route tests for resolution, mismatch, workflow refusal, RLS, rollback, status | PASS |
| AC-019: real client/server journey proves both qualifying grants, every excluded path, idle behavior, suspension/deletion, A/B versions, shared replicas, and no heartbeat | production paths exist for API key/JWT bearer; SQL gate exists | only API-key activation is driven through the real server; the rest is direct issuer/SQL coverage | FAIL — TASKREV-004 |
| AC-020: supporting Postgres integration tests cover the binding/auth seams and directly protect material regressions | new SQL/auth/server tests exist | no same-name/version cross-space credential lookup test and no conditional Operator permission test | FAIL — TASKREV-001, TASKREV-002 |
| AC-028 binding-ID portion: Card GET returns stable binding IDs and preserves read authorization/tenancy | Card hydration reads IDs in the same tenant transaction | route test covers reapply, under-privileged read, and cross-tenant read | PASS |
| AC-030 relevant auth/audit portion: binding registration enforces and audits each required permission | existing cards-write audit remains transactional | no `operators:invoke` decision exists | FAIL — TASKREV-001 |
| Constraint: no Vala control schema/repository, raw pool API, manual tenant predicate, or callee commit | one `wyrd` table and `TenantConn` query owner | boundary checks recorded in task evidence; source inspection | PASS |
| Non-goal: no activity table, heartbeat, activation endpoint, idle refresh, per-request touch, backfill, or second binding identity | activity is one new column on existing principal state; no such mechanism in diff | diff inspection | PASS |
| Ponytail: no speculative runtime, scheduler, dispatch, or Vala consumer added before its owning task | change stops at binding projection/activity/status; downstream tables and runtime remain in TASK-004/005/007 | diff inspection | PASS |

## Proposed Findings

### TASKREV-001 — `INCORRECT` — Operator-bearing bindings bypass `operators:invoke`

- **Violated obligation:** REQ-145, INV-007, AC-030, and TASK-003's mapped auth/audit acceptance require composite registration to spend `cards:write` and, whenever a binding names any `on_failure` Operator, also spend and transactionally audit `operators:invoke`.
- **Exact location:** `crates/wyrd/wyrd-server/src/components/cards/routes.rs:385-405` and `:727-741`; the binding validation/projection path at `crates/wyrd/wyrd-server/src/components/cards/service.rs:545-575,1120-1125` receives only the `card:write` allowed event.
- **Evidence:** `register_card_http` unconditionally calls only `allow_card_write(... Permission::card_write())`. The complete candidate contains no `Permission::operator_invoke()` check in the Cards registration path. The under-privileged case at `pg_card_registration_route.rs:1109-1129` uses a principal with no roles and an empty `on_failure`, so it proves only `cards:write`, not the additional permission.
- **Observable consequence:** a principal granted `cards:write` but denied `operators:invoke` can register a binding that freezes an Operator for later execution. No allow/deny audit row records the required Operator authorization decision.
- **Required testable correction:** before committing a fresh Operator-bearing registration, require `Permission::operator_invoke()` in addition to `cards:write` and append both allowed decisions in the registration transaction; audit a refusal and leave no Card/principal/binding rows. Add public-route cases proving cards-write-only succeeds for an empty `on_failure`, fails for inline and referenced Operators, and a principal with both permissions succeeds with exactly one audit decision per permission.

### TASKREV-002 — `REGRESSION` — relaxing principal-name uniqueness makes exact Card-bound lookup ambiguous

- **Violated obligation:** REQ-105/107/112 and the task outcome require activation and credential issuance for the exact Card-bound Service or Agent version. Security authority requires client assertions not to select a different identity.
- **Exact location:** `crates/wyrd/wyrd-sql/migrations/20260601000027_verification_bindings.sql:19-34` drops tenant-wide name uniqueness; `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:10-30,175-200` still relies on that removed uniqueness and resolves a containment match with `ORDER BY ... LIMIT 1`. Callers are `wyrd-auth/src/issue_api_key.rs:86-99` and `wyrd-auth/src/jwt_bearer.rs:149-158`.
- **Evidence:** `CardRef.space` is optional on the wire (`wyrd-spec/src/reference.rs:14-37`). After this migration, two active principals may share kind/name/version in different spaces. A CardRef omitting `space` matches both JSONB `card_ref` values; the query silently returns the oldest row. Its own source comment states the dropped uniqueness was what bounded the match to one row. The added A/B test covers different versions in one space and cannot expose this path.
- **Observable consequence:** API-key issuance or workload `jwt-bearer` exchange can bind to and activate the wrong Service/Agent Card version when the same named/versioned Card exists in multiple spaces, violating exact-owner activity and issuing authority for the wrong principal.
- **Required testable correction:** make the shared CardRef-to-principal lookup return only one exact identity and fail closed on an underspecified or ambiguous reference; do not restore the old name constraint because concurrent A/B versions are required. Add API-key and workload-`jwt-bearer` integration cases with identical kind/name/version in two spaces, proving a space-pinned reference selects the intended principal and an omitted ambiguous space issues no credential/token and updates neither owner's activity.

### TASKREV-003 — `INCORRECT` — owner bindings do not use the approved subject-occurrence key

- **Violated obligation:** REQ-104 and TASK-003 Scenario 1 define the natural key as `(tenant, owner_card_uid, subject_occurrence_key, verifier_uid)` and require a reserved owner value for Service-level and standalone-Agent bindings; only component bindings use their alias.
- **Exact location:** `crates/wyrd/wyrd-sql/migrations/20260601000027_verification_bindings.sql:39-43,58,71-72`; `crates/wyrd/wyrd-server/src/components/cards/service.rs:1152-1159,1188-1191`; `crates/wyrd/wyrd-sql/tests/pg_verification_bindings.rs:58-69,143-146`.
- **Evidence:** the column is nullable, the unique constraint uses `NULLS NOT DISTINCT`, and owner/Agent sites project `subject_occurrence_key: None`. The tests encode that implementation rather than the approved reserved owner value.
- **Observable consequence:** the durable natural key does not match the approved contract and downstream binding/status/runtime queries must special-case SQL `NULL` instead of consuming one total occurrence-key domain. It also leaves the reserved owner/component-alias boundary unenforced.
- **Required testable correction:** represent every owner occurrence with one internal reserved non-null value that cannot collide with a valid component alias, keep component aliases unchanged, and migrate the constraint/projection tests to prove Service-level, Agent, and component identities remain stable and distinct under reapply and reorder.

### TASKREV-004 — `MISSING` — AC-019's required real-client activity journey is not present

- **Violated obligation:** AC-019, AC-020, AGENTS.md's user-journey rule, and TASK-003 Scenarios 2–3 require real client/server evidence for both qualifying grants and every listed excluded/expiry/lifecycle boundary.
- **Exact location:** `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs:2749-3020`; `crates/wyrd/wyrd-auth/src/issuance.rs:930-1110`; `crates/wyrd/wyrd-sql/tests/pg_verification_bindings.rs:275-480`.
- **Evidence:** the public server test performs one API-key exchange and checks schedule arming. Workload `jwt-bearer`, delegation, human refresh, Card-free automation, cached bearer use, idle expiry/no-refresh, observations, suspension/deletion, A/B versions, and shared-replica behavior are either exercised only by direct issuer/SQL calls or not exercised. Those lower tiers cannot satisfy AC-019's explicit journey requirement. The task evidence also names tests without preserving their exact executed `mise exec -- cargo nextest ... -E 'test(=...)'` commands.
- **Observable consequence:** the credential entry paths and request/client lifecycle that decide whether activity is touched can regress while all new tests remain green; in particular, the candidate has no end-to-end proof that workload assertion verification reaches the qualifying write or that ordinary cached-token traffic does not.
- **Required testable correction:** add the smallest real SDK/client-to-server journey that drives API-key and workload `jwt-bearer` exchange plus the enumerated non-qualifying and lifecycle paths, asserting `last_authenticated_at`, cursor stability/no-backfill, exact-version/replica behavior, and absence of activity writes. Record and run the exact focused selectors required by the task, then the owning integration lanes.

## Open Questions

- None. Each proposed correction stays within approved revision 33 and does not require a new product, public API, persistence, or security decision.

## Verification Notes

- Confirmed the worktree `HEAD` was the immutable candidate before review.
- Inspected the complete diff and the affected registration, principal lookup, API-key, workload assertion, Card status, migration, SQL, and test call paths.
- `git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..467ea07d94a5a6d665d24f56afc0bc0a12532304` passed.
- The task records broad lanes as passing, but its command evidence does not include the exact focused commands for each newly named test. No test lane was rerun during this static review.

## Overall Result

**FAIL**

The candidate implements the central binding projection/activity shape, but it does not satisfy TASK-003 exactly: Operator-bearing registration is under-authorized, the required A/B uniqueness relaxation makes existing exact-principal lookup ambiguous, the durable natural key uses `NULL` instead of the approved owner occurrence value, and AC-019's required journey proof is incomplete.
