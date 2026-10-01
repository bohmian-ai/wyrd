# TASK-007 Delivery and Persistence Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `2bd4ded8f212f7885b0dd78bcd450fa0ff118f3e`
- Reviewed task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35

The candidate remained at the stated commit throughout this review.

## Reviewed boundary

This review traced the Operator delivery boundary from failed-run settlement through frozen dispatch creation, cross-tenant due discovery, tenant-RLS claim and fenced settlement, per-tenant/global permits, per-attempt Operator and connection resolution, credential opening, provider send, retry/deadline handling, key rewrap, shutdown, and capability supervision. It also inspected the Operator-connection migration and SQL owner, key-source configuration and boot wiring, relevant runtime defaults, and the Postgres/server journeys that exercise RLS, multi-replica rotation, fan-out, retry, deadline, fencing, fairness, shutdown, and restart.

## Authority and source coverage

| Boundary | Authority | Source and proof inspected | Result |
|---|---|---|---|
| Tenant persistence and RLS | `AGENTS.md` §§2–3, 6, 9, 11–12; `architecture/agent-rules.md`; REQ-139/147/152; INV-007 | migration `20260601000032_operator_connections.sql`; `operator_connections.rs`; `pg_operator_connections.rs`; route and delivery journeys | PASS |
| Durable dispatch/fencing/clock | REQ-097/098/099/138/142/146/152; AC-029/030/033; analytical reliability reference | `verifier_runs.rs`; `operator_dispatches.rs`; `pg_verifier_runs.rs`; `pg_operator_delivery.rs` | PASS, subject to DR-DEL-3 |
| Cross-tenant discovery and tenant execution | `agent-rules.md` OperatorPool/TenantConn rules; security posture tenant isolation | `OperatorDispatchQueue::due_tenants`; `OperatorWorker::claim_round`/`claim`; queue isolation and fairness tests | PASS |
| Credential lifecycle and KEK readiness | REQ-139/147; TASK-007 Scenario 3; security posture cryptography/secret handling | `config.rs`; `boot/mod.rs`; `components/operators/keys.rs`; connection route and rotation journeys | FAIL (DR-DEL-1, DR-DEL-3) |
| HTTP authority and SSRF ordering | REQ-149; TASK-007 acceptance criterion; `agent-rules.md` SSRF rules; security posture Source credentials and SSRF defense; `wyrd-design.md` Operator delivery | `OperatorWorker::credential`; `OperatorDelivery::send`/`http`/`client`; `HttpRequest::render`; provider journey | FAIL (DR-DEL-2) |
| Retry, deadlines, attempts, and restart | REQ-142/146/152; AC-029/030/033 | runtime defaults; dispatch SQL; slow endpoint, deadline, stale-fence, shutdown, and crash-restart tests | PASS |
| Frozen context and destination | REQ-138/142; INV-011/013 | dispatch insert SQL; `OperatorFailureContext`; template validation/rendering; frozen-context SQL test | PASS |
| Worker supervision and telemetry | REQ-115/146; reliability reference | `verification/mod.rs`; `OperatorWorker::run`; health/restart/metrics assertions | PASS, subject to DR-DEL-3 |

Applicable authorities read for this lane: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`, `architecture/wyrd-doctrine.mdx`, `architecture/references/architecture/patterns.md`, `architecture/references/domain/analytical-operations-reliability.md`, and the Rust, testing, and spec-driven-development references.

## Material proposed findings

### DR-DEL-1 — VIOLATION: multi-tenant production starts without proving the active tenant KEK is readable

- **Violated obligation:** REQ-147 and TASK-007 Scenario 3 require multi-tenant production to use the external provider and fail startup when it or the active 32-byte tenant key is unavailable. Candidate code and candidate architecture may not redefine the approved behavior.
- **Exact location:** `crates/wyrd/wyrd-server/src/config.rs:1830-1835,1872-1915,2835-2841`; `crates/wyrd/wyrd-server/src/boot/mod.rs:1247-1339,1445-1456`; `architecture/wyrd-design.md:1383-1392`.
- **Evidence:** configuration validation checks only that the selected source is Vault and that its address/token fields are present. Boot merely constructs `OperatorKeys` and stores it in `AppState`; no boot path calls `OperatorKeys::key` for the active version. The only production key reads are later create/update, delivery, or rewrap operations. The candidate also changes `wyrd-design.md` to say an unreadable key refuses only credential writes, directly contradicting approved REQ-147 and the original task's startup scenario.
- **Reachable consequence:** a multi-tenant production deployment with an invalid Vault token, unreachable Vault, missing tenant/version path, malformed base64, or a non-32-byte active key becomes ready. Its Operator control plane and every affected delivery then fail only after users rely on the deployment, instead of failing startup as approved.
- **Testable correction:** restore the approved startup contract in `wyrd-design.md` and make production multi-tenant boot validate the configured external provider and active 32-byte key for the tenant key set before readiness. Add a production-profile boot test using a local resolver/Vault fixture that proves missing, unavailable, malformed, and wrong-size active keys fail boot and a readable key permits boot. Development and explicitly single-tenant behavior must retain the approved env/file allowances.

### DR-DEL-2 — VIOLATION: HTTP credentials are attached before the effective URL is SSRF-screened and pinned

- **Violated obligation:** REQ-149, TASK-007's acceptance criterion, `architecture/agent-rules.md`, and `wyrd-design.md:1398-1402` require every effective URL/redirect to pass authority and resolve-screen-pin checks before credentials are attached.
- **Exact location:** `crates/wyrd/wyrd-server/src/verification/operators.rs:663-692,747-785,819-838,854-927`.
- **Evidence:** `OperatorDelivery::send` calls `HttpRequest::render` with the decrypted secret. `HttpRequest::render` inserts bearer/basic/custom-header credentials into its `HeaderMap` at lines 882-907. Only afterward does `OperatorDelivery::http` call `client`, which resolves, screens, and pins the effective URL. On a redirect the credential-bearing `HeaderMap` is retained while the next URL is screened. Thus authority matching occurs before decryption, but the required screen-before-attachment ordering does not.
- **Reachable consequence:** a blocked or unresolved effective destination, and every redirect destination, causes Wyrd to materialize a credential-bearing request before the trust-boundary decision that is required to precede attachment. The current screened client prevents transmission in the inspected path, but the explicit security invariant is absent and future request instrumentation/error handling receives a secret-bearing request too early.
- **Testable correction:** keep the rendered destination and noncredential request separate, resolve-screen-pin the effective URL first, and attach the already-authorized credential only to the request sent through that screened client; repeat that order for each redirect. Add focused proof that a blocked initial URL and a rejected redirect reach no credential-attachment/send boundary, while an allowed pinned URL receives the correct credential.

### DR-DEL-3 — INCORRECT: key rewrap runs inline before claims and is not time-bounded

- **Violated obligation:** REQ-147 requires bounded tenant-scoped rewrap work; REQ-098/115/146 require the generic Operator worker and external delivery to remain bounded and available; the task requires independent durable delivery.
- **Exact location:** `crates/wyrd/wyrd-server/src/verification/operators.rs:217-250`; `crates/wyrd/wyrd-server/src/components/operators/keys.rs:294-395`.
- **Evidence:** every worker replica awaits `rewrap_pass` in the sole Operator claim loop before `claim_round`. A tenant transaction locks up to 100 stale rows, then awaits the active external key and separately awaits the old external key once per row. Vault reads have a ten-second timeout, so one pass can spend repeated network waits while holding row locks and accepting no dispatch claims. The current rotation journey uses one row and local files and does not exercise this path.
- **Reachable consequence:** after key activation, a tenant with many stale rows or a slow external provider can stall the process's only Operator worker long enough for unrelated due dispatches—including other tenants' work—to miss the five-minute delivery deadline without an attempt. Multiple replicas reduce likelihood but do not establish the per-process or cross-tenant bound, because each performs the same inline pass.
- **Testable correction:** isolate rewrap scheduling from the delivery claim loop and place a finite operation/time bound on each tenant pass while preserving TenantConn, SKIP LOCKED, and secret-version fencing. Avoid repeated external reads of the same old key version within one bounded pass. Add a multi-tenant runtime test with stale rows and a slow/unavailable local key resolver that proves due delivery continues within its claim/deadline bound while rewrap remains retryable and bounded.

## Verification limits

- I inspected the candidate's recorded all-green verification evidence and the relevant SQL/server test bodies, but did not rerun Cargo-backed lanes within this reviewer's hard 20-minute budget.
- The gated live Slack/PagerDuty smoke remains unrun because no release credentials were available; this is an explicit release-evidence gap, not an additional finding.
- Existing proof covers RLS, ciphertext-only rows, rotation visibility on another replica, dispatch fencing, Postgres-owned clocks, retry schedules, deadline exhaustion, shutdown release, capability restart, and per-tenant/global permit bounds. It does not cover production boot with an unavailable active external key, screen-before-credential-attachment ordering, or delivery progress during a slow multi-row rewrap pass.
- I did not audit CRUD authorization/audit semantics or SDK/CLI/MCP contract parity beyond their interactions with this persistence/delivery boundary; those belong to other review lanes.

## Overall result

**FAIL** — DR-DEL-1, DR-DEL-2, and DR-DEL-3 are bounded implementation gaps in required startup security, SSRF credential ordering, and delivery availability.
