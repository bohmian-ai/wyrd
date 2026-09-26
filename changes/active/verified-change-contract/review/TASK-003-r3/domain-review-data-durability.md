# TASK-003 R3 Data and Durability Domain Review

## Reviewed Boundary

- Immutable base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Cumulative candidate: `a5a5b60f446981760ac831f64aba871582ec45e4`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- Prior reviews and remediations: `review/TASK-003-r1/` and
  `review/TASK-003-r2/`, including both verdicts, validated finding ledgers,
  and remediation tasks
- Domain scope: persistent control data, SQL and forced-RLS query behavior,
  migrations, caller-owned transactions, concurrency and idempotency, binding
  identity and frozen data, principal activity and schedule cursors, and the
  cumulative completion-gate evidence applicable to TASK-003

The complete original-base-to-candidate diff was reviewed. The data path was
traced from composite Card registration through `persist_node`,
`BindingProjector`, and `project_bindings`; from API-key issuance, workload
`jwt-bearer`, delegation, and the test credential helper through the shared
CardRef principal lookup; and from qualifying tenant token issuance through
`record_machine_authentication` and `binding_activity`. `.codegraph/` is
absent, so caller coverage used repository search and direct source inspection.

## Authority and Source Coverage

| Authority or source | Coverage |
|---|---|
| `AGENTS.md` sections 2-6 and 9-12 | Durable server ownership, typed identities, `TenantConn` composition, RLS, concurrency, test tiers, and completion gates. |
| `architecture/agent-rules.md` | Forced RLS as the sole tenant authority, no manual tenant predicates on `TenantConn`, caller-owned transactions, connection boundaries, and exact verification commands. |
| `architecture/wyrd-design.md` and `architecture/wyrd-doctrine.mdx` | Exact Card-version identity, Card-bound principal lifecycle, binding direction, composite registration, idempotency, and server-owned durable behavior. |
| `architecture/wyrd-security-posture.md` | Fail-closed exact-principal resolution, tenant derivation from verified credentials, forced-RLS tenant isolation, and token-issuance lifecycle. |
| `architecture/v1/00-foundations/sql-foundation.md` | Tenant-qualified keys, forced RLS, transaction ownership, migration immutability, and SQL verification. |
| `architecture/references/architecture/patterns.md` | Registry/storage ownership and tenant-scoped SQL composition. |
| `architecture/references/languages/rust-core.md` | Typed durable identities and dependency-owning workflow structure. |
| `architecture/references/languages/testing-workflows.md` and `spec-driven-development.md` | Postgres integration proof, exact test selection, cumulative remediation review, and evidence authority. |
| Specification REQ-078, REQ-095, REQ-102, REQ-104-108, REQ-112, REQ-134; INV-001, INV-007, INV-010; AC-018-020, AC-028, AC-030 | Binding persistence and identity, exact qualifying activity, monotonic renewal, cursor semantics, A/B versions, tenant isolation, status, and required proof. |
| Original task and both prior review rounds | Original acceptance boundary; R1 findings 1-11; R2 reopening of finding 9 and new findings 12-13; both bounded remediation contracts and their recorded evidence. |
| Cumulative migration and production source | Migration 27; `queries/verification.rs`; shared CardRef lookup and all callers; registration projection; tenant token issuance; activity admission reads; Card-status hydration. |
| Cumulative tests and gate evidence | Full `pg_verification_bindings.rs`; relevant auth, registration-route, identity, SDK, and OpenAPI tests; R1/R2 evidence tables; independently rerun focused lookup and tenant-isolation checks; exact cumulative diff check. |

## Persistent-Data and Query-Seam Assessment

| Seam | Result | Evidence |
|---|---|---|
| Migration and tenant boundary | PASS | Migration `20260601000027_verification_bindings.sql:53-101` creates the tenant-qualified binding table, tenant-qualified owner FK, natural-key constraint, indexes, enabled and forced RLS, canonical `wyrd.current_tenant()` policy, and least-required runtime grants. `auth_service_accounts` was already forced-RLS in migration 1. Dropping tenant-wide name uniqueness is bounded by the retained exact Card-binding unique key and a partial unique index for Card-free names. |
| Registration transaction | PASS | `cards/service.rs:1121-1176` writes the Card, relationships, principal, and binding projection on one caller-owned `TenantConn`; `BindingProjector` at `:1179-1235` and `project_bindings` at `queries/verification.rs:251-301` never commit. Existing rollback and cross-tenant tests exercise this seam. |
| Stable binding identity and frozen data | PASS | The durable natural key is exactly `(data_tenant_id, owner_card_uid, subject_occurrence_key, verifier_uid)`; `$owner` is non-null and disjoint from validated component aliases; first insert mints UUIDv7 and conflict returns the stored identity. Referenced Trigger/Operator UIDs, inline digests, activation, and schedule are frozen on the row. Reapply, reorder, changed Verifier, owner/component, Agent-owner, and invalid stored UUID-version cases are covered in `pg_verification_bindings.rs`. |
| Exact-one CardRef lookup | PASS | `service_accounts.rs:26-33,185-201` fetches at most two active containment matches and returns a row only when exactly one exists. No `ORDER BY` fallback remains. API-key issuance, workload `jwt-bearer`, CardRef delegation, and `WyrdTestServer::credential_registered_service` all use this shared lookup, so explicit references resolve and ambiguous partial references fail through the callers' existing refusal paths. |
| Forced RLS after tenant-predicate removal | PASS | The shared CardRef lookup now binds only principal kind and CardRef; it contains no `data_tenant_id` predicate or tenant bind. Its `TenantConn` transaction activates the forced policy on `wyrd.auth_service_accounts`, leaving one tenant authority. The focused SQL-text test pins `$1`/`$2`, `LIMIT 2`, absence of `data_tenant_id`, and absence of `$3`; `mise run check:tenant-isolation` passes. |
| Activity durability and concurrency | PASS | `RECORD_AUTHENTICATION_SQL` uses Postgres `GREATEST(last_authenticated_at, $2)` while updating only an active Card-bound Service/Agent principal. Competing issuers serialize on that principal row, so an older supplied server timestamp cannot replace a newer stored one. The reverse-order test proves the stored timestamp, full activity window, and cursor stability. |
| Cursor arming and idempotency | PASS | After the principal update, unarmed schedule rows are locked in binding-ID order and only `next_run_at IS NULL` rows are updated. The first qualifying exchange arms the next future boundary; later exchanges renew activity without moving an armed cursor; `observations_ready` rows remain cursor-free. The shared principal row and binding-row locking order is consistent across issuers. |
| Admission read | PASS | `BINDING_ACTIVITY_SQL` resolves the exact binding owner Card and Card-bound principal under RLS, requires both lifecycle states active, and compares the stored activity stamp with the supplied timeout cutoff. Component bindings inherit the containing Service principal; A/B Card UIDs use distinct principals; suspension/deletion is visible on the next read. |
| Migration compatibility and no duplicate state | PASS | Existing principals retain null activity until a qualifying exchange. No heartbeat, activity table, backfill, activation endpoint, per-request touch, Vala control store, or second binding identity was added. Only the binding table needed by TASK-003 is introduced; later control tables remain owned by their consuming tasks. |

## Prior-Finding Closure

| Finding | R3 assessment | Current evidence |
|---|---|---|
| `FIND-TASK-003-2` | **CLOSED** | The shared lookup selects exactly one active match or returns `None`; all production credential/delegation callers still route through it. Multi-space ambiguity no longer selects by creation order. |
| `FIND-TASK-003-3` | **CLOSED** | The database update is monotonic with `GREATEST`; reverse-order exchanges cannot shorten activity or reset the null-only cursor. |
| `FIND-TASK-003-4` | **CLOSED** | Effective schedule validation calls `next_after` before registration writes; impossible dates are refused rather than persisted as permanently unarmable bindings. |
| `FIND-TASK-003-5` | **CLOSED** | The persisted occurrence key is non-null, owner bindings use `$owner`, component aliases reject the reserved value, and Agent rows are constrained to the owner occurrence. |
| `FIND-TASK-003-6` | **CLOSED** | Public verification SQL boundaries use `PrincipalId`, `BindingId`, and `CardUid`; private raw rows validate UUIDv7 binding/Card identities before return. |
| `FIND-TASK-003-7` | **CLOSED** | The dependency-backed freeze/project workflow belongs to the transaction-scoped `BindingProjector`; narrow SQL operations and pure conversions remain direct. |
| `FIND-TASK-003-12` | **CLOSED** | The exact-one lookup's manual `data_tenant_id` predicate and bind are removed. Forced RLS on the supplied `TenantConn` is now the sole tenant authority, with active filtering and ambiguity refusal unchanged. |
| `FIND-TASK-003-13` | **CLOSED** | Only trailing spaces were removed from the preserved R1 standards report; the exact cumulative `git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..a5a5b60f446981760ac831f64aba871582ec45e4` exits zero. |

The R1 verdict was `FIX_REQUIRED` with findings 1-11. The R1 remediation closed
the data findings above plus the adjacent authorization, journey, contract,
OpenAPI, and documentation gaps. The R2 verdict was `FIX_REQUIRED` for reopened
finding 9 and new findings 12-13. The R2 remediation changes are bounded to the
TypeScript status projection, removal of the lookup's duplicate tenant
authority, and whitespace-only gate repair; none regresses the persistent-data
closures established in R1.

## Material Findings

None. No reachable persistent-data, migration, transaction, RLS, exact-owner,
concurrency, idempotency, or cursor defect remains within the approved TASK-003
boundary. Optional hardening beyond the server-owned typed write and forced-RLS
paths was not elevated into a finding.

## Verification Evidence and Limits

- Independently passed the exact focused lookup test:
  `mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'`.
- Independently passed `mise run check:tenant-isolation`.
- Independently passed the exact cumulative gate:
  `git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..a5a5b60f446981760ac831f64aba871582ec45e4`.
- The complete Postgres and caller test bodies were inspected. The cumulative
  implementation records report the named Postgres tests, `test:sql`, auth and
  Card journeys, SDK journeys, RLS/boundary checks, format, lints, and codegen
  passing. Those broader lanes were not rerun in this Wave-1 review.
- The reverse-order activity test uses two committed tenant transactions in
  reverse timestamp order rather than orchestrating simultaneous blocked
  tasks. That is sufficient for the relevant database property because
  `GREATEST` is evaluated by the later update after row serialization; direct
  inspection confirms the common principal-then-binding lock order.
- SYSTEM issuance remains a later-task path and does not exist in this
  candidate. TASK-003 adds no route by which it could write activity.

## Overall Result

**PASS**

The cumulative candidate satisfies TASK-003's persistent-data, SQL/RLS,
migration, transaction, concurrency, idempotency, exact binding identity,
activity, cursor, and applicable completion-gate obligations. The assigned
prior findings are closed and no new material domain finding is proposed.
