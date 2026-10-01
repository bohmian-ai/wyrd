# TASK-007 R6 Delivery Durability Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `cd002ab1394cdc1679d95f957313a7f697d7e0a5`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 36
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Remediation inputs: TASK-007-R1 through TASK-007-R5 and the R1-R5 review ledgers

The candidate remained the stated repository `HEAD` before and after this
review. The repository has no `.codegraph/` index, so source and caller tracing
used the immutable checkout directly.

## Reviewed boundary

This review traced the cumulative delivery path from failed-run settlement and
transactional dispatch creation through cross-tenant due discovery,
permit-before-claim admission, tenant-scoped leased claims, fenced settlement,
per-attempt Operator and credential resolution, provider delivery, bounded
response and redirect handling, retry/deadline arithmetic, independent sibling
status, shutdown release, crash/restart recovery, and bounded key rewrap.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Failed-only durable handoff and independent fan-out | Spec `REQ-097`-`REQ-099`, `INV-011`; TASK-007 Scenario 6 | `verifier_runs.rs` `COMPLETE_RUN_SQL`/`INSERT_DISPATCH_SQL`; dispatch schema, uniqueness, RLS, and settlement tests | PASS |
| PostgreSQL clock, leases, fencing, attempt budget, and deadline | Spec `REQ-142`, `REQ-146`, `REQ-152`, `INV-015`, `AC-033`; `AGENTS.md` durability and tenant rules | `operator_dispatches.rs` claim/exhaust/deliver/fail/retry/release statements and `OperatorDispatchQueue`; `pg_verifier_runs.rs` fence, deadline, release, sibling, and maximum-delay tests | PASS |
| R5 maximum decimal `Retry-After` settlement | R5 `FIND-TASK-007-19`; spec `REQ-142`, `REQ-146`, `REQ-152` | `RETRY_SQL` bounds the requested milliseconds with the existing five-minute deadline before interval multiplication, then clips to the absolute database-owned deadline; focused Postgres regression | PASS / CLOSED |
| Tenant isolation and persistent state | Spec `REQ-098`, `REQ-139`, `REQ-147`, `INV-007`; `agent-rules.md` `TenantConn`/`OperatorPool` boundary | Forced-RLS dispatch/connection tables; tenant-scoped claim, settlement, connection lookup, and rewrap; admin pool limited to due/discovery metadata | PASS |
| Permit-before-claim capacity and fairness | Spec `REQ-146`, `AC-030`; TASK-007 Scenario 6 | `claims.rs` `ClaimLoop::claim_round` acquires global/per-tenant capacity before the committed claim; fairness journey | PASS |
| Provider protocols and transport bounds | Spec `REQ-099`, `REQ-140`-`REQ-142`, `REQ-149`, `AC-029`; SSRF rules in `agent-rules.md` | `OperatorDelivery`, private Slack/PagerDuty modules, screened/pinned per-hop clients, attachment ordering, 30-second timeout, bounded redirects/body, status classification, and local provider journeys | PASS |
| Retry, shutdown, crash, and restart behavior | Spec `REQ-142`, `REQ-146`, `AC-030`; TASK-007 Scenario 7 | `OperatorWorker::process`/`settle`, `ClaimLoop::run`, runtime supervision, release/refund SQL, slow-endpoint/shutdown/restart journeys | PASS |
| Credential rotation and rewrap isolation | Spec `REQ-139`, `REQ-147`; prior `FIND-TASK-007-9` | Fresh connection lookup/decryption per attempt; independently budgeted and cancellable rewrap pass; rotation/outage/slow-rewrap journeys | PASS |

## Prior-finding closure

| Finding | Current evidence | Result |
|---|---|---|
| `FIND-TASK-007-1`-`FIND-TASK-007-3` | Selector-safe key failures, production readiness, and screen/pin-before-attachment remain in the cumulative source and focused proofs. | CLOSED |
| `FIND-TASK-007-5`, `FIND-TASK-007-7` | Production Vault transport and owner-only/nonblocking file reads retain the reviewed constraints. | CLOSED |
| `FIND-TASK-007-9` | Rewrap discovery and tenant work remain time-bounded, cancellable, and independent of delivery claims. | CLOSED |
| `FIND-TASK-007-10`-`FIND-TASK-007-13` | Exact key-version handling, redacted key owners, private provider modules, and truthful PagerDuty semantics remain intact. | CLOSED |
| `FIND-TASK-007-18` | Production environment-key rejection is outside this delivery-focused review; the cumulative configuration correction and focused proof are present for the security/configuration reviewers to validate. | DEFERRED TO SECURITY/CONFIGURATION DOMAIN |
| `FIND-TASK-007-19` | `operator_dispatches.rs:112` bounds the accepted delay before PostgreSQL interval construction; `maximum_retry_after_settles_at_the_deadline` proves `u64::MAX` seconds schedules exactly at the database deadline and clears the lease. | CLOSED |

## Material proposed findings

None. The reviewed delivery boundary contains no material acceptance,
durability, concurrency, tenancy, provider-bound, or regression finding.

## Verification notes and limits

- `git diff --check` passed for the complete base-to-candidate range.
- Ran the exact focused Postgres-backed expression for
  `maximum_retry_after_settles_at_the_deadline`,
  `dispatch_delivery_obeys_budget_deadline_and_fencing`, and
  `expired_dispatch_deadline_fails_without_a_claim`: **3 passed**.
- Inspected the cumulative SQL and real-server journeys for fan-out, provider
  refusal/rate limiting, credential rotation/outage, fairness, timeout,
  shutdown release, worker restart, tenant isolation, and rewrap isolation.
  This time-bounded review did not rerun the broad SQL/server suites.
- The maximum-delay regression enters at the queue boundary with the exact
  `Duration` produced by the accepted decimal parser. It directly proves the
  repaired persistent-state arithmetic; the existing provider journey retains
  the ordinary `Retry-After: 90` parser-to-settlement path.
- Credentialed Slack/PagerDuty live smoke remains intentionally gated release
  evidence and was not run. Local mock-provider coverage proves the reviewed
  protocol and settlement behavior without external credentials.

## Overall result

**PASS** — the cumulative candidate satisfies the reviewed Operator delivery
durability, concurrency, lease/fencing, retry/deadline, Postgres/RLS,
crash/restart/drain, and provider-protocol obligations. R5 closes
`FIND-TASK-007-19` without changing public contracts or adjacent lifecycle
semantics, and no new delivery finding remains.
