# TASK-007 R4 Delivery, Durability, and Persistence Domain Review

## Immutable review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `e2da27694e8a3d057f2ff863c5d17429adc0a495`
- Scope: the complete cumulative base-to-candidate change, including the
  original task and the R1, R2, and R3 remediation packets and review ledgers.
- Candidate identity was checked before inspection and immediately before this
  report was written. `HEAD` remained the required candidate.

## Reviewed boundary

This review traced the complete durable delivery path for TASK-007:

- forced-RLS Postgres connection persistence, ciphertext-only storage,
  same-identity credential rotation, disable/re-enable, version-fenced DEK
  rewrap, and production key readiness;
- failed-run settlement and same-transaction fanout of one immutable dispatch
  per distinct frozen Operator identity;
- cross-tenant due discovery, permit-before-claim admission, tenant fairness,
  short tenant transactions, database-clock availability/deadlines, lease
  fencing, retry/exhaustion, stale-worker settlement, restart, and shutdown;
- per-attempt resolution of the current connection and credential, with no
  process credential cache and independent sibling outcomes;
- Slack `chat.postMessage`, PagerDuty Events API v2, and authored HTTP delivery,
  including response truth, timeout/backoff limits, bounded response handling,
  stable dispatch identity, redirect screening, and credential attachment only
  after resolve-screen-pin;
- the R3 correction for exact key-version construction and API-role ownership.

## Authority and source coverage

The controlling material inspected was `AGENTS.md`,
`architecture/agent-rules.md`, `architecture/wyrd-design.md`,
`architecture/references/languages/spec-driven-development.md`,
`architecture/references/languages/testing-workflows.md`,
`architecture/references/domain/analytical-operations-reliability.md`, approved
specification revision 35, the original TASK-007 file, all three remediation
tasks, and the prior delivery/security/task/standards reports, validated
ledgers, and verdicts.

| Domain obligation | Source and caller coverage | Result |
|---|---|---|
| REQ-097–099, REQ-138: atomic failed-result fanout and immutable bounded context | `wyrd-sql/src/queries/verifier_runs.rs` completion and dispatch insertion; dispatch migration; runner settlement callers; `pg_verifier_runs.rs`; `pg_operator_delivery.rs` | **PASS** — only a completed failed binding run creates dispatches; insertion shares the settlement transaction, is idempotent by frozen identity, and stores only the approved context. |
| REQ-098, REQ-142, REQ-146, REQ-152, INV-015: durable claims, database clock, capacity, retries, restart, and drain | `operator_dispatches.rs`; `verification/claims.rs`; `verification/operators.rs`; runtime supervision and health; SQL and server delivery tests | **PASS** — both permits precede the short committed claim, every transition is lease-token fenced, PostgreSQL owns due/deadline decisions, retries are clipped to the five-minute deadline, and shutdown releases unstarted/abandoned work under the same identity. |
| REQ-139, REQ-147, REQ-149, INV-006–007: encrypted tenant credential lifecycle and per-attempt authority | migration and `operator_connections.rs`; connection service/key owner; `wyrd-crypt`; registration compatibility path; `OperatorWorker::credential`; route and delivery integration tests | **PASS** — forced RLS and tenant-scoped transactions protect rows; Postgres stores envelope material rather than plaintext; each attempt reloads the latest active exact-authority row before opening it; key rewrap is version fenced and does not decrypt the credential. |
| REQ-140–142: Slack, PagerDuty, and HTTP delivery truth | `verification/operators.rs`; private `slack.rs` and `pager_duty.rs`; screened HTTP owner; mock-provider and focused classification tests | **PASS** — Slack checks JSON `ok`; PagerDuty sends the route, exact subject source, severity, summary, and stable dedup key without claiming guaranteed grouping; HTTP screens and pins every effective hop before attaching matching credentials, rejects origin changes, owns `Idempotency-Key`, and classifies retryable versus terminal outcomes within the attempt/deadline ceilings. |
| REQ-146: fair independent work and maintenance | shared claim loop and permit owner; separately joined delivery and rewrap futures; due-tenant ordering; fairness, slow-provider, stalled-discovery, and shutdown/restart tests | **PASS** — delivery capacity is globally and per-tenant bounded, one saturated tenant does not block another, rewrap does not consume delivery permits, and both discovery and tenant rewrap work share a cancellable absolute pass budget. |
| R3 `FIND-TASK-007-10`: exact key-version and role-owned construction | all `OperatorKeys::new` and `OperatorKeys::for_role` callers; boot state attachment; config validation; test-server construction; delivery error mapping | **PASS** — direct construction returns selector-free `KeyError::VersionOutOfRange`; API-bearing roles validate/build the configured owner; the Forge-only role retains the unused default without reading irrelevant key configuration; `i32::MAX` remains exact. |

The path is reachable end to end: failed result settlement creates committed
dispatch rows; the supervised worker discovers due tenants, acquires capacity,
claims and commits a fenced lease; it resolves the frozen Operator and latest
matching credential under the run tenant; the provider adapter executes under
the database-returned remaining budget; and a tenant transaction records the
single token-fenced delivery, failure, retry, or release transition. No direct
Verifier-to-provider path, second broker, Alert persistence, or credential
cache entered the cumulative diff.

## Prior-finding closure

All delivery-relevant prior findings remain closed in the cumulative candidate:

- `FIND-TASK-007-1`–`-3`, `-5`, and `-7` remain closed by selector-safe key
  failures, production key readiness, per-hop screen/pin before attachment,
  HTTPS/owner-only Vault access, and nonblocking mounted-file reads.
- `FIND-TASK-007-9` remains closed because delivery and bounded key rewrap are
  independent and the absolute pass budget includes cross-tenant discovery.
- `FIND-TASK-007-10` is now closed at both normal boot and direct-constructor
  boundaries; no oversized external key version can narrow or unwind.
- `FIND-TASK-007-11`–`-12` remain closed by redacted key-owner diagnostics and
  focused provider wire modules.
- `FIND-TASK-007-13` is closed: PagerDuty documentation now describes only
  possible downstream grouping and explicitly disclaims an exactly-once or
  incident-grouping guarantee.

No corrected path regressed the durable claim, credential, provider, or rewrap
behavior.

## Verification evidence

Fresh review execution on the immutable candidate:

- Exact `wyrd-server` focused tests for the oversized constructor, role-owned
  construction, API-role configuration refusal, exact `i32::MAX`, and key
  diagnostic redaction: **4/4 passed**.
- `git diff --check f8811ac5035c3aa165d34c38992f9889b3c9081f..e2da27694e8a3d057f2ff863c5d17429adc0a495`:
  **passed**.
- Candidate identity recheck: **passed**.

The R3 implementation record additionally reports green `fmt`, `lints`,
`test:wyrd` (including both Postgres Operator delivery/connection suites),
`test:sql`, and `check:unwrap-audit`. Prior immutable review evidence covers the
focused Postgres deadline/fencing, expiry, rotation/rewrap, stalled discovery,
slow rewrap, fairness, provider outcome, shutdown, and restart cases. These
recorded broader results were treated as available evidence rather than rerun
in this domain review.

## Verification limits

- The complete broad workspace gate and all language SDK journeys were not
  rerun; the R3 delta is confined to typed key-version refusal, role-owned boot
  construction, documentation, and import shape, and its exact behavioral
  tests were rerun.
- Credentialed Slack and PagerDuty release smoke remains gated external
  evidence and was not run. Local provider journeys exercise the request,
  response, retry, and failure contracts without production credentials.
- CodeGraph was unavailable because this checkout has no `.codegraph/`
  directory; source, SQL, tests, and callers were traced directly.

These limits do not prevent a delivery-domain conclusion: the cumulative
durability/concurrency path has direct source and Postgres journey evidence,
and the only delivery-adjacent R3 behavioral change passed its focused proof.

## Material proposed findings

None.

## Overall result

**PASS** — the cumulative candidate satisfies the approved durable connection,
fanout, claim/fencing, retry/deadline, restart/shutdown, rewrap, Slack,
PagerDuty, and HTTP delivery obligations in this sensitive domain. No bounded
implementation correction is required.
