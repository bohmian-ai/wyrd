# TASK-007 R2 Delivery, Concurrency, and Persistence Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `b50ce7bc45c67f62dc7dabe34a2a1ef1ed421c34`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Remediation task: `changes/active/verified-change-contract/review/TASK-007-r1/TASK-007-R1-operator-delivery-corrections.md`

The candidate remained at the stated commit throughout this review. Review
artifacts are outside the immutable subject.

## Reviewed boundary

This review traced the cumulative base-to-candidate delivery path from failed
Verifier-run settlement and transactional fan-out through Postgres due-tenant
discovery, tenant-RLS `SKIP LOCKED` claims, lease-token fencing, permits,
provider sends, retry/deadline settlement, shutdown/restart, and status. It also
traced Operator-connection ciphertext persistence, per-attempt latest-secret
resolution, production key readiness, key-version discovery, bounded rewrap,
concurrent replica behavior, and old-key reuse. The prior R1 delivery findings
and their remediation were reassessed against the actual cumulative source and
tests rather than the implementation summary.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Failed-only transactional fan-out | REQ-097/099/138; INV-011/013 | `wyrd-sql/src/queries/verifier_runs.rs` completion and dispatch SQL; verifier-run and delivery Postgres tests | PASS |
| Forced-RLS connection persistence | REQ-139/147; INV-007; `AGENTS.md` SQL rules; `architecture/agent-rules.md` | migration `20260601000032_operator_connections.sql`; `operator_connections.rs`; connection SQL and route journeys | PASS |
| Durable claim, lease, fence, and database clock | REQ-098/142/146/152; AC-030/033 | `operator_dispatches.rs`; shared `verification/claims.rs`; stale-lease, retry, deadline, crash/restart, and shutdown tests | PASS |
| Independent delivery concurrency | REQ-098/146; architecture delivery contract | `OperatorWorker`, `ClaimLoop`, `VerifierPermits`; per-tenant/global fairness journey | PASS |
| Per-attempt credential and provider truthfulness | REQ-099/139/140/141/142/149; AC-029/031; security posture SSRF rules | `OperatorWorker::credential`; `OperatorDelivery`; screened/pinned redirect tests; mock-provider journeys | PASS |
| Production KEK readiness and redaction | REQ-139/147; R1 `FIND-TASK-007-1`, `-2`, `-5`, `-7` | `boot::verify_operator_keys`; `OperatorKeys`; Vault HTTPS/token-file checks; selector-safe errors; focused unit tests | PASS |
| Rewrap scheduling, cancellation, elapsed bounds, and old-key reuse | REQ-142/147/146; R1 `FIND-TASK-007-9`; `wyrd-design.md` Operator connection and delivery sections | `OperatorWorker::rewrap`; `OperatorKeys::rewrap_pass`/`rewrap_tenant`; `referenced_key_versions`; slow-rewrap journey | **FAIL (`DR-DEL-R2-1`)** |

Applicable authorities inspected for this lane were `AGENTS.md`,
`architecture/agent-rules.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-security-posture.md`,
`architecture/references/architecture/patterns.md`,
`architecture/references/languages/rust-core.md`,
`architecture/references/languages/testing-workflows.md`,
`architecture/references/languages/spec-driven-development.md`, and
`architecture/references/domain/analytical-operations-reliability.md`.

## Prior relevant finding closure

| Prior finding | Closure status | Evidence |
|---|---|---|
| `FIND-TASK-007-1` | CLOSED | `KeyError` retains only source kind, version, and stable failure class; public detail is constant and the focused selector/log test passed in this review. |
| `FIND-TASK-007-2` | CLOSED | Multi-tenant production API boot calls `verify_operator_keys`, enumerates active tenants through `OperatorPool`, and reads/decodes each active version before readiness. |
| `FIND-TASK-007-3` | CLOSED | HTTP request rendering contains no credential; every effective URL is screened and pinned before `Credential::attach`, including redirects. Both focused attachment-order tests passed in this review. |
| `FIND-TASK-007-5` | CLOSED | Production configuration rejects plaintext Vault URLs and Vault token files reuse the owner-only reader before network access. |
| `FIND-TASK-007-7` | CLOSED | Mounted key and token reads use the shared `spawn_blocking` owner-only boundary; the focused single-thread executor test passed in this review. |
| `FIND-TASK-007-9` | **NOT CLOSED** | Rewrap no longer precedes delivery and old versions are reused within the tenant batch, but the cross-tenant discovery query remains outside enforcement of the pass deadline (`DR-DEL-R2-1`). |

## Material proposed finding

### DR-DEL-R2-1 — INCORRECT: the rewrap pass deadline does not bound cross-tenant discovery

- **Violated obligation:** REQ-147 requires bounded tenant-scoped rewrap work;
  the R1 correction and `wyrd-design.md` require finite per-pass and per-tenant
  elapsed-time bounds with cancellation. This is the remaining unclosed part
  of `FIND-TASK-007-9`.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/operators/keys.rs:506-539` and
  `crates/wyrd/wyrd-sql/src/queries/operator_connections.rs:327-348`.
- **Evidence:** `rewrap_pass` computes its `deadline`, then awaits
  `referenced_key_versions(operator)` without a timeout or PostgreSQL
  `statement_timeout`. The remaining pass budget is checked only after that
  query returns. `OperatorPool` supplies an acquire timeout but no execution
  deadline for this query. Consequently a lock wait or stalled database
  statement can exceed `RewrapBudget::pass` indefinitely. The existing
  `slow_rewrap_never_holds_back_another_tenants_delivery` test stalls Vault
  reads after discovery and therefore does not exercise this path.
- **Observable consequence:** delivery remains available because rewrap now
  runs beside the claim loop, but the promised rewrap pass bound is false: a
  replica can retain a rewrap operation beyond its configured two-minute pass
  budget, rotation progress can stop on discovery, and interval scheduling
  cannot begin its next bounded pass until the database call eventually
  returns or shutdown cancels it.
- **Required testable correction:** apply the existing pass budget to the
  complete rewrap pass, including `referenced_key_versions`, while preserving
  the current per-tenant transaction timeout, `TenantConn`, `SKIP LOCKED`,
  secret-version fence, selector-safe logging, and per-batch old-key reuse. A
  pass timeout must cancel the in-flight discovery future and return control to
  the interval loop without affecting delivery; no new scheduler or cache is
  needed.
- **Focused closure proof:** hold the connection table behind a database lock
  (or use an equivalent deterministic stalled-discovery fixture), run a pass
  with a short budget, and prove it returns within that budget with no row
  mutation; then release the stall and prove a later pass completes rewrap.
  Keep the existing slow-provider/other-tenant delivery test as the broader
  availability proof.

## Verification evidence and limits

- Ran the exact focused `wyrd-server` unit selection for selector-safe key
  failures, owner-only Vault token reads, nonblocking mounted-key reads, blocked
  and unresolved HTTP destinations, and per-redirect credential attachment:
  **5 passed**.
- Inspected the cumulative SQL migration and query owners, the shared claim
  loop, Operator worker/delivery and key owner, runtime supervision, boot
  readiness, and the relevant Postgres journeys.
- The candidate records the broader SQL, server, integration, SDK, codegen,
  boundary, formatting, and lint lanes as passing. This time-bounded domain
  review did not rerun those broad lanes or the credentialed Slack/PagerDuty
  release smoke.
- No existing test stalls cross-tenant key-version discovery, so the pass-bound
  defect is established from the reachable SQL wait path and missing deadline,
  not from a reproduced timeout test.
- CodeGraph was unavailable because this repository has no `.codegraph/`
  index; source and caller tracing used the repository files directly.

## Overall result

**FAIL**

The cumulative candidate preserves the required RLS persistence, atomic
fan-out, fenced delivery, provider success boundaries, concurrency limits,
retry/deadline behavior, and delivery availability during slow key-provider
rewrap. One bounded correction remains: the configured rewrap pass deadline
does not include its cross-tenant discovery query, so prior
`FIND-TASK-007-9` is not fully closed.
