# TASK-003 Data and Durability Domain Review

## Reviewed Boundary

- Immutable base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- Immutable candidate: `467ea07d94a5a6d665d24f56afc0bc0a12532304`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- Domain scope: persistent control data, migrations, registration/auth transaction composition, RLS, binding natural-key identity, frozen referenced/inline identities, reapply/reorder/version behavior, activity-update races, schedule-cursor initialization, eligibility reads, and migration compatibility.

The review traced the registration write path through `wyrd-server` and `wyrd-sql`, the shared token-issuance activity write through `wyrd-auth`, and every changed Postgres migration/query and relevant Postgres test in the cumulative diff.

## Authority and Source Coverage

| Authority or source | Coverage |
|---|---|
| `AGENTS.md` §§2-6, 9, 11-12 | Durable server ownership, `TenantConn` composition, forced RLS, Rust/async rules, and required verification tiers. |
| `architecture/agent-rules.md` | Caller-owned transaction lifecycle, no manual tenant predicates on `TenantConn`, forced tenant isolation, and registry transaction/test rules. |
| `architecture/wyrd-design.md` | Card-bound principal identity, exact CardRef credential binding, verification binding model, and composite registration transaction. |
| `architecture/wyrd-doctrine.mdx` | Durable exact-version references and server-owned verification behavior. |
| `architecture/wyrd-security-posture.md` | Exact Card-bound principal lifecycle, tenant isolation, and token issuance identity. |
| `architecture/references/architecture/patterns.md` | SQL owner and transaction-composition boundaries. |
| `architecture/references/languages/rust-core.md` | Typed durable identities, `TenantConn`, and transaction ownership. |
| `architecture/references/languages/testing-workflows.md` | Postgres integration and user-journey proof requirements. |
| `spec.md` REQ-078, REQ-104-108, REQ-112; AC-018-020, AC-028, AC-030 | Binding persistence, UUIDv7/natural-key stability, exact-owner activation, monotonic renewal semantics, cursor arming, A/B versions, and tenant isolation. |
| Candidate migration and SQL/auth/registry source | Full changed migration and query bodies, registration projection, token issuance call path, status hydration, and relevant surrounding principal lookup code. |
| Candidate tests | `pg_verification_bindings.rs`, changed auth Postgres tests, and the registration-route journey additions, including their uncovered concurrency and multi-space cases. |

## Review Findings

### Critical

None.

### Important

#### DATA-001 — Relaxing principal-name uniqueness makes CardRef lookup ambiguous across spaces

- **Classification:** REGRESSION / INCORRECT
- **Violated obligation:** REQ-105 requires authentication to activate the exact Card-bound owner; REQ-108 requires exact-version independence; the security/design authorities require API-key issuance and workload binding to resolve the requested Card-bound principal, while `CardRef.space` remains optional at the wire boundary.
- **Location:** `crates/wyrd/wyrd-sql/migrations/20260601000027_verification_bindings.sql:19-29`; `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:20-39`; consumers at `crates/wyrd/wyrd-auth/src/issue_api_key.rs:92-99` and `crates/wyrd/wyrd-auth/src/jwt_bearer.rs:149-158`.
- **Evidence:** The migration drops `UNIQUE (data_tenant_id, name)` for all Card-bound principals and replaces it only for Card-free rows. The existing lookup uses `card_ref @> $3`, orders matches by creation time, and returns one row. Its own source documentation states that an omitted space matches every space and that the removed unique constraint was what bounded the lookup to one row; it explicitly warns that relaxing that constraint requires narrowing the predicate. Two Service Cards with the same name and version in different spaces are now valid rows, and a CardRef without `space` matches both. API-key issuance and workload `jwt-bearer` resolution both consume this lookup.
- **Observable consequence:** A request for one Card can issue a credential for, or authenticate as, the oldest same-named Card in another space. The resulting token carries that other Card's scope, and TASK-003 records activity and arms bindings for that other exact owner. This defeats the exact-owner activation contract and makes behavior depend on row creation order.
- **Required testable correction:** Preserve A/B versions while making Card-bound lookup unambiguous. Resolve/default the requested space before selecting a principal (or otherwise require the complete exact Card identity) and remove the oldest-row fallback as an identity selector. Add a Postgres/auth test with same-kind, same-name, same-version Cards in two spaces proving that an omitted-space request resolves according to the canonical space rule and an explicit-space request binds only that exact Card; prove both API-key issuance and workload binding cannot select the sibling space.

#### DATA-002 — Out-of-order concurrent exchanges can move `last_authenticated_at` backwards

- **Classification:** INCORRECT / CONCURRENCY
- **Violated obligation:** REQ-105 requires every later successful qualifying exchange to renew activity; REQ-106 requires persistence of the last successful qualifying exchange time; REQ-107 derives eligibility from that value; REQ-108 requires replicas sharing one principal to share correct activity without postponing an armed cursor.
- **Location:** `crates/wyrd/wyrd-sql/src/queries/verification.rs:49-57` and `:317-348`; call site `crates/wyrd/wyrd-auth/src/issuance.rs:349-353`.
- **Evidence:** Each replica captures `Utc::now()` before entering `record_machine_authentication`, and the SQL unconditionally assigns `last_authenticated_at = $2`. Postgres serializes the row updates, but it does not order the supplied application timestamps. An exchange that captured an older time can wait behind and then overwrite a newer committed timestamp. The existing renewal test (`crates/wyrd/wyrd-sql/tests/pg_verification_bindings.rs:308-336`) exercises only ordered calls in one transaction and cannot expose this path.
- **Observable consequence:** A successful authentication can shorten, rather than renew, the owner's activity window. Eligibility may expire early for every replica and component binding sharing that principal. The already-armed cursor remains stable, so the row can present a cursor based on the first successful exchange but stale activity based on a later-finishing older timestamp.
- **Required testable correction:** Make the principal activity update atomic and monotonic in Postgres, retaining the later of the stored value and the supplied successful-exchange time while preserving the existing null-cursor-only arming behavior. Add a focused Postgres test that applies newer and older exchange timestamps in reverse completion order (preferably with two tenant transactions), then proves `last_authenticated_at` remains the newer value, eligibility remains active for the corresponding full timeout, and `next_run_at` is not reset.

### Suggestions

None.

## Verification Notes and Limits

- Independently passed: `mise run check:tenant-isolation`.
- Independently passed: `mise run check:registry-tx-coupling`.
- Independently passed: `git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..467ea07d94a5a6d665d24f56afc0bc0a12532304`.
- Independently passed the four focused `wyrd-sql` schedule/timeout unit tests through exact `cargo nextest` expressions under `mise exec --`.
- The task packet records passing SQL, auth, card-registration, journey, lint, codegen, and boundary lanes. Those full Postgres/journey lanes were not rerun in this review.
- Existing tests prove forced RLS, rollback coupling, UUIDv7 IDs, reorder/reapply stability, A/B version independence, cursor arming, and ordered renewal. No reviewed test covers same-name/same-version principals across spaces after the uniqueness migration or out-of-order/concurrent activity timestamps; both findings remain reachable despite the reported green lanes.

## Overall Result

**FAIL**

The binding projection itself is transactionally composed and tenant-isolated, but exact principal resolution regresses when the migration removes the uniqueness assumption used by credential lookup, and the activity timestamp is not monotonic under concurrent replica exchanges. Both defects violate TASK-003's exact-owner and renewal semantics.
