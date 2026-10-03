# TASK-008 round-six repository standards review

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `1d05642bf2c4d824de1aec27286048ec79b37e25`
- Candidate: `f8d6945041467d52020024d311ce8831a45ff4a1`
- Reviewed range: `1d05642bf2c4d824de1aec27286048ec79b37e25..f8d6945041467d52020024d311ce8831a45ff4a1`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review and remediation:
  `changes/active/verified-change-contract/review/TASK-008-r5/`

This is the user-directed closure review. I reviewed only whether this range
closes `FIND-TASK-008-CLOSEOUT-16` and
`FIND-TASK-008-CLOSEOUT-17`, and whether the range introduces a repository-rule
regression. Earlier accepted code was not reopened. `FIND-13` remains deferred
to integration. The separately tracked intermittent
`verification_runtime::two_bindings_share_one_client_observation` failure is
not charged to this range.

The checkout has no `.codegraph/` directory, so navigation used Git, `rg`, and
direct source inspection. Review was static only; the review orchestrator owns
all Cargo and `mise` verification for this shared checkout.

## Navigation map

| Changed surface | Owner and role | Callers or consumers inspected | Proof inspected |
|---|---|---|---|
| `wyrd-server/src/oracle/query_audit.rs` | `OracleQueryAudit` owns the process-local non-blocking audit queue and `OracleAuditWriter` owns its commit loop | `AppState::audit_outbox`, Oracle `OracleAudit`, direct verification staging, server shutdown | New gauge transitions; held-chain-head capacity proof; existing publication and shutdown paths |
| `wyrd-testing/src/bin/capacity/evidence.rs` | `Queue` reads durable backlog; `Backlog` combines durable and replica-local evidence | `Deployment::drain`, report records, capacity evidence tests | Held pending-to-staging-to-publication proof and arithmetic unit proof |
| `wyrd-testing/src/bin/capacity/step.rs` | `Deployment` owns replicas, clients, queue, and measured-step lifecycle | `Benchmark::clean_up`, `Deployment::run`/`drain` | Slow-replica cleanup proof and exact 60-second drain test source |
| `wyrd-testing/src/bin/capacity/main.rs` | `Benchmark` owns setup-to-report and client-before-replica cleanup ordering | Capacity entry point and cleanup tests | Updated owner-path slow-stop proof |
| `architecture/bifrost-design.md` | Bifrost read-audit and telemetry authority | Oracle audit implementation and capacity scrape | Gauge meaning checked against producer lifecycle |
| R4 remediation artifact | Active remediation/evidence record | Round-six reviewers and eventual completion review | Acceptance mapping and recorded command set |

## Authority coverage

| Changed surface | Applicable authority read and applied |
|---|---|
| All changed Rust and tests | `AGENTS.md` sections 4-6, 11, 12, 15, and 16; `architecture/agent-rules.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/maintainer-style.md`; `architecture/references/languages/testing-workflows.md` |
| Audit producer, metric, and durable handoff | `AGENTS.md` sections 2, 3, 9, and 10; `architecture/agent-rules.md` audit rules; `architecture/bifrost-design.md` read-audit, resource, and telemetry sections; `architecture/wyrd-design.md` runtime identity/audit boundary; `architecture/wyrd-security-posture.md` tenant and audit sections; `architecture/references/architecture/patterns.md`; `architecture/references/languages/agent-harness.md`; `architecture/references/domain/olap-serving.md`; `architecture/references/domain/analytical-operations-reliability.md`; `architecture/operations/reliability-and-recovery.md`; `architecture/operations/deployment-and-release.md` |
| Capacity evidence and benchmark proof | Spec revision 57 `REQ-171`; original TASK-008 closeout; R5 verdict, findings validation, and R4 remediation; `TESTING.md`; `mise.toml`; `architecture/references/languages/spec-driven-development.md`; testing and analytical-reliability references above |
| Architecture documentation edit | `AGENTS.md` architecture-authority rules; `architecture/bifrost-design.md`; reference router and the audit/telemetry references above |

The pre-existing security-posture prose describing an Oracle local fsynced
acceptance record conflicts with the current locked `AGENTS.md`, agent-rule,
and Bifrost single-outbox/no-second-WAL contract. This range neither introduces
nor changes that older prose, and the user-directed scope forbids reopening it.
It does not make this range's benchmark accounting false because the current
runtime source has only the process-local `OracleQueryAudit::pending` owner
before canonical staging.

## Applicable rule results

| Rule | Source evidence | Result |
|---|---|---|
| Stateful, multi-step replica shutdown must live on its concrete owner | `step.rs:38-56,427-454` places collection shutdown on `Deployment`; `main.rs:589-604` preserves `Benchmark`'s client-before-replica transition; the free workflow is deleted | PASS; closes the repository-structure part of FIND-16 |
| Blocking process termination must not occupy a Tokio worker | `step.rs:431-450` retains `spawn_blocking` around `LocalServer::stop` and documents cancellation/partial progress | PASS |
| Async must directly await IO or compose operations that do | `Deployment::stop_replicas` awaits the blocking process-stop join; `Deployment::drain` awaits metrics and Postgres reads; pure backlog combination remains synchronous | PASS |
| Every materially modified Rust item requires substantive rustdoc, and every fallible operation requires `# Errors` | Changed constants, fields, helpers, tests, gauge lifecycle, and cancellation behavior are documented; `Deployment::stop_replicas` returns per-replica process/join errors but has no `# Errors` section | **FAIL — `REPO-R6-001`** |
| Oracle/direct-verification decisions remain non-blocking and use the one canonical staging path and publisher | `query_audit.rs:107-116,162-215` changes only a mirror gauge around the existing bounded queue and `append_audit_batch`; no request wait, second writer, WAL, relay, table, or publisher is added | PASS |
| The process-local metric must match the real owner without a handoff gap | `query_audit.rs:107-115` increments before enqueue and rolls back on refusal; `query_audit.rs:176-178` decrements only after commit or counted loss | PASS; closes the pre-commit part of FIND-17 |
| Durable audit backlog must cover every unpublished staged row, including a post-stop commit | `evidence.rs:257-300` removes the stop-time upper bound only for audit staging and retains the per-tenant publication-watermark join | PASS; closes the post-commit part of FIND-17 |
| Replica-local evidence must be sampled before durable evidence to avoid a false zero | `step.rs:406-418` scrapes replicas before `Queue::backlog`; `evidence.rs:182-200` sums pending plus staged and permits only conservative double counting | PASS |
| Metrics use bounded labels and do not expose tenant identity or secrets | `audit_outbox_pending` is an unlabeled process gauge; architecture documents its process-owned meaning | PASS |
| Tenant isolation and typed SQL ownership must not be weakened | Production audit commits still acquire `ValaPostgres::tenant_conn(tenant)` and call the canonical append; no raw production pool or manual tenant predicate was introduced | PASS |
| Benchmark evidence must preserve the exact REQ-171 drain edge and report meaning | The drain still delegates the decision to `Drain::judge`; only the audit evidence union changes; no report column, workload, SLO, or deadline changes occur in the range | PASS |
| Postgres/live-server tests belong in `mod pg_tests` or a `pg_*` test file, outside the ordinary fast-test module | `evidence.rs:336-566` adds an ignored Postgres-backed `WyrdTestServer` test directly inside `#[cfg(test)] mod tests` | **FAIL — `REPO-R6-002`** |
| Specifically named tests require exact repository-pinned commands and environment ownership | The R4 evidence records exact `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity` selectors, with the repository Postgres wrapper for the ignored handoff proof, plus the owning broader lanes | PASS structurally; execution is left to the orchestrator |
| No gate may be weakened to clear a failure | The new integration proof is ignored only as the repository's environment-gated execution mechanism and has an explicit Postgres command; no assertion, test, boundary check, or timeout was weakened | PASS |
| Generated/public language contracts require their owning codegen or journey proof | No schema, OpenAPI, SDK, Python, TypeScript, MCP, or generated artifact changed | NOT APPLICABLE |
| Public errors, secrets, SSRF, and external network validation | No public error, secret-bearing type, or tenant-controlled fetch path changed | NOT APPLICABLE |

## Material findings

### `REPO-R6-001` — `Deployment::stop_replicas` omits mandatory error documentation

- **Violated rule:** `AGENTS.md` section 16 and
  `architecture/agent-rules.md` require every new or materially modified
  fallible Rust operation to document its error conditions in a `# Errors`
  section; missing rustdoc is `BLOCK_BEFORE_MERGE`.
- **Location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:427-454`.
- **Evidence:** the new owner method returns
  `Vec<std::result::Result<f64, String>>`; each element can contain a
  `LocalServer::stop` failure or a `spawn_blocking` join/panic failure. Its
  prose mentions a panic conversion, but it contains no `# Errors` section and
  does not describe both error sources under the repository's required
  fallibility contract.
- **Consequence:** FIND-16's workflow now has the correct owner, but the newly
  materialized owner operation remains incomplete under the repository's hard
  Rust documentation gate. A maintainer cannot rely on the standard error
  section to distinguish process-stop failures from blocking-task join
  failures.
- **Testable correction:** add a substantive `# Errors` section to the method
  documenting that results preserve per-ordinal `LocalServer::stop` failures
  and blocking-task join/panic failures. No signature, behavior, or test change
  is required.

### `REPO-R6-002` — the new Postgres/server proof is in the ordinary test module

- **Violated rule:** `architecture/agent-rules.md` and `AGENTS.md` testing
  rules require tests needing Postgres, Docker, or a live server to live in
  `mod pg_tests` or a `pg_*` file rather than the ordinary fast-test module.
- **Location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:336-566`, especially
  `the_audit_backlog_holds_from_a_pending_decision_until_its_publication` at
  lines 442-566.
- **Evidence:** the test starts a bound `WyrdTestServer`, uses the fixture
  database, acquires a tenant transaction lock, and drives the real publisher,
  but is declared inside `#[cfg(test)] mod tests`. `#[ignore]` keeps it out of
  the default execution, but does not satisfy the repository's structural
  tier classification rule.
- **Consequence:** the owning source no longer makes the environment boundary
  discoverable through the mandated `pg_tests`/`pg_*` shape, and future lane
  selection can mistake a database/server test for an ordinary capacity unit
  test. This regression is introduced by the range even though the recorded
  focused command supplies the correct Postgres wrapper.
- **Testable correction:** move only the Postgres/server helpers and held-audit
  test into an in-source `pg_tests` module (or an earned `pg_*` integration
  target) while leaving the pure metric arithmetic tests in `tests`; update
  the exact focused selector and rerun it through the existing repository
  Postgres wrapper.

## Finding closure and regression assessment

- `FIND-TASK-008-CLOSEOUT-16`: **behaviorally and structurally closed** by the
  `Deployment` owner method, subject to the new rustdoc standards finding
  `REPO-R6-001`.
- `FIND-TASK-008-CLOSEOUT-17`: **closed**. The pending gauge and scrape-before-
  durable-read ordering cover the pre-commit owner, and the watermark query
  covers late committed rows through publication without changing audit
  request latency or durable ownership.
- No correctness, tenancy, durability, security, metric-cardinality, or
  benchmark-report regression was found in the range.
- The known intermittent verification-runtime journey failure is noted as a
  separate integrated-branch blocker and is not counted here.

## Overall result

**FAIL**

The two prior findings are substantively closed, but the remediation range
introduces two bounded hard repository-rule violations in documentation and
test-tier placement. Both can be corrected without changing approved behavior
or production semantics.
