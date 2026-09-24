# TASK-007 r5 — Delivery Durability Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `1e857c89f116fc7901e365bc456db6a20cbea684`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 36, user-approved 2026-09-24
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Remediation inputs: TASK-007-R1 through TASK-007-R4
- Candidate remained at the stated commit throughout this review.

## Reviewed boundary

This review traced failed-run settlement through transactional dispatch fan-out,
PostgreSQL due discovery and permit-before-claim admission, leased/fenced claim
and settlement, per-attempt Operator and credential resolution, Slack,
PagerDuty, and HTTP delivery, retry/deadline handling, independent sibling
status, tenant fairness, rewrap isolation, supervision, shutdown release, and
restart recovery.

## Authority coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Failed settlement and durable handoff | Spec REQ-097/098/099, INV-010/011 | `verifier_runs.rs` `COMPLETE_RUN_SQL`, `INSERT_DISPATCH_SQL`, and `VerifierRunQueue::complete`; verifier-run/dispatch migration; dispatch fan-out SQL tests | PASS |
| Database clock, leases, fencing, and status | Spec REQ-146/152, INV-015, AC-033; `AGENTS.md` SQL/async rules; `agent-rules.md` `TenantConn`/`OperatorPool` rules | `operator_dispatches.rs` claim/exhaust/deliver/fail/retry/release SQL; `DispatchLease`; `pg_verifier_runs.rs` deadline/fence/sibling tests | **FAIL** (`DELIVERY-1`) |
| Permit-before-claim, fairness, and capacity | Spec REQ-146, AC-030; architecture runtime ownership | `claims.rs` `ClaimLoop::claim_round`/`claim`; `permits.rs`; `operator_permits_cap_each_tenant_without_starving_another` | PASS |
| Credential lookup and rotation | Spec REQ-139/147/149; `wyrd-design.md` Operator connection contract; `wyrd-security-posture.md` source credential boundary | `OperatorWorker::credential`; `OperatorKeys::key/open/rewrap_pass`; connection SQL; rotation/outage/rewrap journeys | PASS |
| Provider delivery truth and ambiguity | Spec REQ-099/140/141/142, AC-029 | `OperatorDelivery`; `slack.rs`; `pager_duty.rs`; mock-provider fan-out, provider-refusal, timeout, redirect, and rotation journeys | PASS except the retry-header defect in `DELIVERY-1` |
| HTTP SSRF and secret attachment ordering | Spec REQ-149; `agent-rules.md` SSRF rule; `wyrd-security-posture.md` resolve-screen-pin sequence | `ScreenedHttp::client_for`; `OperatorDelivery::http/post_json`; focused blocked, unresolved, and per-redirect credential-attachment tests | PASS |
| Shutdown, restart, and health | Spec REQ-115/146, AC-030 | `ClaimLoop::run`; `VerificationRuntime::run`; worker crash/restart and shutdown-release journey | PASS |
| Testing obligations | `AGENTS.md` test taxonomy; testing-workflows reference; TASK-007 scenarios 6–7 | SQL transition tests, real-server mock-provider journey, concurrency, restart, shutdown, rewrap, and ignored live-provider smoke | PASS with limits below |

## Material proposed findings

### `DELIVERY-1` — Unbounded `Retry-After` can make retry settlement fail

- **Classification:** INCORRECT
- **Violated obligation:** REQ-142 requires bounded `Retry-After` handling inside
  the dispatch deadline; REQ-146 fixes the retry schedule/deadline; REQ-152
  requires PostgreSQL to own the resulting coordination decision.
- **Exact location:**
  - `crates/wyrd/wyrd-server/src/verification/operators.rs:942-960`
    parses any decimal `Retry-After` fitting `u64` into an unbounded
    `Duration`.
  - `crates/wyrd/wyrd-sql/src/queries/operator_dispatches.rs:104-116`
    evaluates `statement_timestamp() + ($5 * INTERVAL '1 millisecond')`
    before `LEAST` can clip it to the five-minute deadline.
  - `crates/wyrd/wyrd-sql/src/queries/operator_dispatches.rs:342-345`
    converts an oversized duration to `i64::MAX` milliseconds.
- **Evidence:** `Retry-After: 18446744073709551615` is accepted by the parser;
  its duration reaches `millis` as `i64::MAX`. An isolated PostgreSQL 17 probe
  of the exact candidate arithmetic,
  `statement_timestamp() + (9223372036854775807::bigint * INTERVAL '1 millisecond')`,
  fails with `ERROR: interval out of range`. The existing journey covers only
  `Retry-After: 90` and therefore does not exercise this reachable response.
- **Observable consequence:** a remote HTTP destination, Slack endpoint, or
  PagerDuty endpoint can return a syntactically valid large delta-seconds
  header and cause `OperatorWorker::settle` to fail. The dispatch remains
  `running` until its 45-second lease expires; reclaim then spends another
  attempt and may repeat an externally ambiguous send instead of honoring the
  provider delay clipped to the five-minute deadline. This bypasses the
  required retry scheduling path and records settlement failure rather than a
  durable bounded retry/terminal transition.
- **Required testable correction:** keep PostgreSQL as the deadline owner, but
  bound the requested delay to the server-owned dispatch deadline (or the
  database-computed remaining interval) **before** interval multiplication in
  `RETRY_SQL`, then retain the absolute-deadline `LEAST` and existing fence.
  Add a focused queue/worker regression using the maximum accepted decimal
  `Retry-After`; it must settle without a SQL error, never schedule beyond the
  database deadline, and never rely on lease expiry to make progress. No new
  retry type, clock abstraction, or dependency is needed.

## Prior-finding closure inspected independently

| Prior finding | Current evidence | Result |
|---|---|---|
| FIND-TASK-007-1 selector-safe key failures | Constant delivery/public error details and selector-free key logging remain in `keys.rs`/error catalog | CLOSED |
| FIND-TASK-007-2 production key readiness | Boot calls `OperatorKeys::verify_active` over the active-tenant directory before readiness for multi-tenant production | CLOSED |
| FIND-TASK-007-3 screen before credential attachment | Initial URL and each redirect build a screened/pinned client before `Credential::attach`; focused tests pass | CLOSED |
| FIND-TASK-007-5 Vault boundary | Production requires HTTPS Vault and owner-only token-file reads | CLOSED |
| FIND-TASK-007-7 nonblocking secret files | File/token reads use `spawn_blocking` through the shared owner-only reader | CLOSED |
| FIND-TASK-007-9 rewrap starvation/budget | Rewrap runs beside claims; discovery and each tenant are bounded/cancellable | CLOSED |
| FIND-TASK-007-10 exact key version | Active version is fallibly converted to `i32` before owner construction | CLOSED |
| FIND-TASK-007-11 redacted key-owner debug | `OperatorKeys` delegates to redacting configuration and the Vault client has no token-bearing defaults | CLOSED |
| FIND-TASK-007-12 provider isolation | Slack/PagerDuty wire construction and Slack reply classification remain in focused private modules | CLOSED |
| FIND-TASK-007-13 PagerDuty truth | Documentation and implementation claim provider acceptance and possible grouping, not exactly-once delivery or incident creation | CLOSED |

## Verification notes and limits

- Ran the exact focused `wyrd-server` nextest expression for Slack reply
  classification and initial/redirect SSRF credential ordering: 3 passed.
- Ran an isolated repository-managed PostgreSQL probe of the candidate's
  maximum-delay interval expression: it failed with `interval out of range`,
  confirming `DELIVERY-1` rather than inferring it.
- Inspected the candidate's SQL integration and real-server Operator journeys,
  including dispatch fencing/deadline, fan-out, provider errors, rotation,
  shutdown, restart, tenant capacity, and rewrap isolation. This reviewer did
  not rerun the full PostgreSQL suites or broad repository gates.
- The credentialed Slack/PagerDuty live smoke is intentionally ignored and was
  not run; live provider acceptance remains release evidence outside the
  credential-free review environment.
- HTTP-date-form `Retry-After` is not covered by existing tests. This report
  does not broaden the approved contract by asserting a required wire syntax;
  `DELIVERY-1` concerns the already-accepted decimal form.

## Overall result

**FAIL** — the cumulative candidate satisfies the reviewed delivery lifecycle
except for `DELIVERY-1`, a reachable bounded-retry/durable-settlement violation.
