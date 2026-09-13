# TASK-001-R2 independent task review

## Subject

- Approved spec: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-001-integrate-redux-data-plane.md`
- Remediation task: `review/task-001-r1-8e61c0349-review-01/TASK-001-R2-close-r1-review-findings.md`
- R2 range: `8e61c0349..0ca4ee7ef`; cumulative remediation range `8377fff9f..0ca4ee7ef`
- Reviewer: fresh independent `task-rev`; the candidate stayed immutable and nothing was committed
- The R2 "Implementation evidence" section was treated as a claim. Every row below comes from source, diff, or a command I ran myself.

## Commands run by this reviewer

```bash
# R1-1 unit (3/3 PASS)
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::tenant_table::tests::tenant_table_binding_admits_system_owner_only_for_audit_log) | test(=scribe::ingress::tests::nil_tenant_frames_admit_only_internal_audit_publication) | test(=tables::audit::projection::tests::projects_system_owner_rows)'
# R1-2 / R1-3 / R1-9 (4/4 PASS)
WYRD_REG_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=bifrost::service::pg_tests::bifrost_tables_register_pre_commit_failure_records_one_verdict) | test(=bifrost::service::pg_tests::bifrost_tables_concurrent_same_fqn_register_records_each_verdict) | test(=components::admin::routes::pg_tests::create_records_allowed_decision_before_failed_discovery) | test(=components::admin::routes::pg_tests::configured_checker_denial_governs_workload_binding_create)'"
# R1-4 / R1-6 (8/8 PASS)
WYRD_REG_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_card_registration_route --test-threads=1 -E 'test(=registration_refuses_when_its_decision_audit_fails) | test(=delete_audit_failure_keeps_card_active) | test(=delete_by_ref_not_found_records_one_decision) | test(=registration_replays_through_public_authenticated_route) | test(=completion_audit_failure_keeps_card_pending) | test(=card_reconciler_dead_letters_after_three_failures) | test(=blob_storage_failure_leaves_durable_failure_state) | test(=delete_storage_failure_preserves_cleanup_state)'"
# R1-1 journey + R1-5 progress (2/2 PASS)
WYRD_REG_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=audit_publication::system_owner_security_rejections_retain_once) | test(=audit_publication::a_stalled_tenant_does_not_block_another_tenants_history)'"
mise run check:skills-sync   # fails on mtime-only drift; file contents are identical (see scope note)
```

I did not re-run the full lanes: `test:bifrost:journey:server`, `:mcp`, `lints`, and the rest.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | PASS/FAIL |
|---|---|---|---|
| R1-1: a system-owner rejection is retained exactly once and system staging drains; non-audit nil ingress still fails | `AuditLogTable::admits_system_owner` (`tables/audit/audit_log.rs:23-35`) is checked at `TenantTableBinding::facts` (`tenant_table.rs:122`) and in Scribe `validate_logical_transport_frame`, which also requires `PLATFORM_AUDIT_PRINCIPAL` (`scribe/ingress.rs:58-61`). The projection nil guard is removed. `LogicalTableIdentity` (Oracle reads only) still rejects nil. | Three unit tests pass, including the non-audit-table and non-publisher-principal negatives. The journey `system_owner_security_rejections_retain_once` passes: +1 Scribe batch fence and drained staging, through the production `PostgresPeerSecurityAudit` and publisher. | PASS |
| R1-2: every permitted Bifrost registration records one verdict, including a pre-append failure and a same-FQN race; a successful create stays transactional | `append_registration_audit` runs before commit on the create path and the concurrent-winner path (`bifrost_catalog.rs:993-995, 1018`). `register_table` records standalone on every non-`AuditUnavailable` catalog error (`bifrost/service.rs:155-176`). | `..._pre_commit_failure_records_one_verdict` and `..._concurrent_same_fqn_register_records_each_verdict` pass. The race test cannot force the concurrent-winner branch, but both branches yield exactly one row. | PASS |
| R1-3: an issuer discovery or sealing failure after Allowed records one event before external IO, creates no issuer, and no transaction spans discovery | `admin/routes.rs:232-278`: local `request_client_auth`, then the verdict, then standalone `record_audit` on the pool, then discovery, then sealing, then a fresh `acquire_conn` insert with no second append | `create_records_allowed_decision_before_failed_discovery` passes: one `allowed` row, no issuer row | PASS |
| R1-4: Card registration and both deletes commit or roll back the Allowed row with the SQL effect; no-write outcomes record one decision | `append_on` is the first statement in `write_registration` and in both delete transactions. `register_card` and `record_unless_committed` record standalone on replay, lost race, or error. The missing-idempotency-key route path records standalone. | Card focused set 8/8 passes: audit-failure refusal, delete audit failure, by-ref not-found (one row), per-request replay decision, completion | PASS |
| R1-5: the production sweep shows unordered progress and **never exceeds the fixed concurrency bound**; the disconnected scaffolding is gone | `InFlight` test deleted (`audit/publication.rs`). No journey drives more than 8 tenants. The journey doc says so: `audit_publication.rs:296-298`. | Only the two-tenant `a_stalled_tenant_...` passes. The ceiling has no executable proof. | **FAIL** (TR-1) |
| R1-6: the R1-edited dead-letter scenario runs with an exact passing command; TASK-002's five lanes stay outside R2 | `wyrd-testing/src/server.rs:3631-3675` gives Local settings the `list_with_start_after` override. The smoke test uses `with_storage_handle`. | `card_reconciler_dead_letters_after_three_failures` passes. **But** commit `662bf33bc` rewrote the five TASK-002 lanes. | **FAIL** (TR-2) |
| R1-7: every touched Rust item meets the bare-type and rustdoc contracts; contradictory prose is aligned | Stale Card and `wyrd-design.md` dead-letter prose is fixed. `ServerGate`, eval `CardRef`, and the audit helper signatures are bare. The test was renamed. | Source inspection finds a new qualified signature and materially changed handlers without `# Errors`. | **FAIL** (TR-3, TR-4) |
| R1-8: valid lifecycle states | TASK-001-R1, TASK-001-R2, TASK-001, TASK-005, and TASK-006 are all `status: review` | Frontmatter grep | PASS |
| R1-9: service-account response, effect, and the single audit row follow the configured `PermissionCheck` | `authorize_service_accounts_write` delegates to `authorize_recording_denial` (`state.authz.permission_check`) and remaps only the RBAC denial to the prior exact message and details (matches `wyrd-auth/src/service_accounts.rs:20-23`). Callers are admin, `auth/revoke.rs`, and `auth/routes.rs`. | `configured_checker_denial_governs_workload_binding_create` passes: exact message, one `denied` row, no binding | PASS |
| R2 required cumulative verification set, plus INV-025 (no accepted baseline failures) | The R2 record lists `test:bifrost:journey:server` and `:mcp` as INTERMITTENT and attributes them to TASK-006. TASK-006 records no such ownership (`TASK-006...md:174` records `journey:server` 7/7). | The R1 record ran both lanes as required. R2 now records them failing (2/3 and 2/4). | **FAIL** (TR-5) |
| Original task: Redux sole engine, RLS/`TenantConn`, gapless chain, frozen range, Scribe fences, Oracle WAL-first, Forge lineage preserved | No R2 change to publication range, Oracle WAL, or Forge lineage. The nil exception is limited to the audit log and the platform audit principal. | Focused journeys above | PASS |
| Non-goals and Ponytail minimum: no new seam, harness, test file, config, dependency, scheduler, or migration; no broadening into TASK-002 | Code changes reuse the existing owners (`append_on`, `record_audit`, `authorize_recording_denial`, `AuditLogTable`, existing fixtures). Two small helpers (`append_registration_audit`, `record_unless_committed`) each remove duplicated call sites and pass the ladder. | The mise/inventory launcher rewrite is TASK-002 scope. | **FAIL** (TR-2) |
| Human-approval disclosure (a) | The only record is the implementer's sentence "Human-approved during implementation" in the task's own evidence (`TASK-001-R2...md:276`). No spec revision, task revision, verdict, or commit message records an approval. Spec revision 7 has no waiver, and INV-025 forbids one. | grep across the change packet and R2 commit messages | Not substantiated (TR-1) |
| Scope of `ebf8dd7ae` | It adds the R1 review packet, the R2 task, and TASK-002's launcher section (the transfer that R1's findings-validation recorded), plus workflow-skill edits. These are review and planning artifacts, not candidate implementation. The `wyrd-design.md` edit belongs to `461f7614c` (R1-7, in scope). `.agents` and `.claude` skill contents are identical; `check:skills-sync` flags only timestamps. | diff / `check:skills-sync` | Not drift for this task |

## Findings

### TR-1 — MISSING: the fixed publication concurrency ceiling is still unproven, and the "approval" is unrecorded

- **Violated obligation:** R2 §5 and its acceptance row R1-5 ("demonstrate that at most eight reach the blocked publication point, an additional tenant waits, and all tenants progress and drain"); R2 focused proof item 6; spec INV-025.
- **Location:** `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:296-298`; `crates/wyrd/wyrd-server/src/audit/publication.rs:55,164`; `TASK-001-R2-close-r1-review-findings.md:245,274-276`
- **Evidence:** No test observes more than two tenants in the production sweep. The deleted unit test was the only ceiling check, so the property now has zero coverage, which is weaker than at R1. The stated blocker is `WYRD_DB_MAX_CONNECTIONS=8`. That is only a default: `scripts/postgres/with-test-postgres.sh:71` reads `${WYRD_DB_MAX_CONNECTIONS:-8}`, so a lane can raise it without a new knob. No human approval appears in the spec, the task, a verdict, or a commit.
- **Observable consequence:** If `PUBLICATION_TENANT_CONCURRENCY` or the `for_each_concurrent` call were changed to unbounded, serial, or a different limit, every recorded test would still pass.
- **Testable correction:** Extend `a_stalled_tenant_does_not_block_another_tenants_history`, or a sibling case in the same file, to fence more than 8 tenants in the existing publication journey. Run it under a pool large enough for the fences, using the existing env default override. Assert that exactly 8 tenant cycles block, the 9th makes no progress while they are held, and all tenants drain after release. Alternatively, record an explicit human-approved spec or task revision that removes the ceiling-proof obligation.

### TR-2 — DRIFT: TASK-002's Postgres launcher repair was implemented in the TASK-001-R2 candidate

- **Violated obligation:** R2 "Constraints": "Do not broaden into … TASK-002's Postgres launcher repair". R2 §6: "Do not repair or require the full Card, CLI, or WyrdState lanes here: TASK-002 owns their stale Postgres launchers". Acceptance row R1-6: "TASK-002's five owned lanes remain outside R2".
- **Location:** commit `662bf33bc`: `mise.toml:98-130` (`test:cards:integration`, `test:cli:journey`, `test:wyrdstate:journey`, plus the new `:inner` tasks), `mise.toml:922-945` (`py:test:cards:integration`, `py:test:wyrdstate:integration`), `scripts/postgres/check-inventory.py:31-32`. It also deletes `build:postgres`, `setup:postgres`, and related tasks.
- **Evidence:** The diff implements TASK-002's "Postgres Test Lifecycle Correction" section almost word for word. The R2 evidence lists no verification for it; `test:postgres:inventory` and the five lanes are not recorded.
- **Observable consequence:** Shared CI and test infrastructure changed without the proof TASK-002 requires, inside a candidate whose review cannot accept that scope. TASK-002's acceptance criterion now appears pre-satisfied by unreviewed work, and the task boundary between TASK-001 and TASK-002 is blurred.
- **Testable correction:** Remove the `mise.toml` and `check-inventory.py` launcher changes from the TASK-001-R2 range and leave them to TASK-002. Alternatively, obtain a recorded plan revision that moves this work into R2, and add passing `mise run test:postgres:inventory` plus the five lane runs as evidence.

### TR-3 — VIOLATION: a new signature uses a qualified cross-tier type path

- **Violated obligation:** R1-7 ("import existing owning types at the module top and use bare names in every touched … parameter"); `architecture/agent-rules.md` bare-name and cross-tier re-export rules (Vala code imports `vala_sql::TenantConn`)
- **Location:** `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1489-1490`: `async fn append_registration_audit(conn: &mut wyrd_sql::TenantConn<'_>, …)`, added in R2 (`4d3d84808`)
- **Evidence:** The item is new in R2 and its parameter uses a fully qualified `wyrd_sql::` path from inside the Vala tier.
- **Observable consequence:** R1-7's bare-type contract is not closed for touched items, and the tier bypasses its own `vala_sql` re-export surface.
- **Testable correction:** Add `use vala_sql::TenantConn;` at the top of the module and declare `conn: &mut TenantConn<'_>`. Confirm with `mise run lints` and a grep showing no `wyrd_sql::TenantConn` in R2-added signatures.

### TR-4 — VIOLATION: materially changed fallible Card HTTP handlers lack `# Errors` and do not describe their new audit behavior

- **Violated obligation:** R1-7 ("add accurate intent and required `# Errors` … to touched items"); AGENTS.md §16; `agent-rules.md` rustdoc rule
- **Location:** `crates/wyrd/wyrd-server/src/components/cards/routes.rs:286-292` (`register_card_http`), `:363-380` (`delete_card_http`), `:404-423` (`delete_card_by_ref_http`)
- **Evidence:** R2 changed all three. They now obtain the Allowed event through `allow_card_write` and hand it to the service transaction, and `register_card_http` adds a new standalone audit record on a missing idempotency key. All three return `Result<…, WyrdErrorResponse>`, and none has a `# Errors` section. The two delete handlers have a single summary line.
- **Observable consequence:** A maintainer cannot see from the docs that these handlers fail with `AuditUnavailable` or record the verdict standalone on the missing-key path. This is a hard blocker under the repository rustdoc contract.
- **Testable correction:** Add rustdoc to the three handlers covering the verdict and audit ordering and a `# Errors` section listing the RBAC denial, `AuditUnavailable`, the missing idempotency key (registration only), and the service failures. Confirm by source inspection.

### TR-5 — MISSING: required journey lanes are recorded as failing, contrary to INV-025

- **Violated obligation:** R2 "Focused proof" (it names `test:bifrost:journey:server` and `test:bifrost:journey:mcp` in the required set); spec INV-025 ("no accepted baseline failures. A failing or unexecuted required test or journey prevents completion"); TASK-001 verification
- **Location:** `TASK-001-R2-close-r1-review-findings.md:272,278-281`
- **Evidence:** `journey:server` failed 2 of 3 runs (`frozen_audit_range_replays_once_while_its_tail_waits`, `a_stalled_tenant_does_not_block_another_tenants_history`). `journey:mcp` failed 2 of 4 (`delegated_agent_query_is_attributed_in_its_durable_audit_record` reads transient staging that the publisher may already have drained). The R1 remediation record lists both lanes in its required, executed command set (`integrated-remediation-01/TASK-001-R1...md:235,240`), and TASK-006 records `journey:server` 7/7. The claimed hand-off to TASK-006 appears in no TASK-006 artifact. The "pre-existing" argument rests on runs with only the R1-1 flag disabled. The other R2 changes to audit volume and timing were still active, so a regression cannot be ruled out. I did not re-run these lanes.
- **Observable consequence:** Required journeys are not green, and INV-025 blocks completion. The MCP test's race against the publisher can fail CI at random.
- **Testable correction:** Fix the root cause at the owning test or publisher timing. For example, the MCP journey should read the retained `vala.system.audit_log` decision, or wait on a deterministic publication condition, rather than read transient staging. The server-lane stalls need the same treatment. Then record repeated green runs of `mise run test:bifrost:journey:server` and `mise run test:bifrost:journey:mcp`. Alternatively, record an explicit, human-approved ownership transfer in the owning task with a failing-test reference.

## Overall result

**FAIL**, with findings TR-1, TR-2, TR-3, TR-4, and TR-5. R1-1, R1-2, R1-3, R1-4, R1-8, and R1-9 are credibly closed, and I verified them with independent focused runs. R1-5 is unproven and its claimed approval is unrecorded. R1-6 is closed for the dead-letter scenario but broke the TASK-002 scope boundary. R1-7 is still partial. The required journey lanes are not green.
