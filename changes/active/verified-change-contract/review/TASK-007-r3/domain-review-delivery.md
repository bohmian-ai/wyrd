# TASK-007 R3 Delivery, Durability, and Persistence Domain Review

## Immutable review subject

- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `57ee8dd0b3dddc3e1da34bf543ecc566bae2e831`
- Scope: the complete cumulative diff, not only the R2 remediation delta.
- Candidate identity was checked before review and again before this report was
  written. `HEAD` remained the required candidate.

## Reviewed boundary

This review covers the durable operator-connection and delivery boundary:

- ciphertext-only connection persistence, atomic credential rotation, disabled
  state, RLS, version fencing, and ciphertext rewrap;
- failed-run fanout and frozen failure context in the verifier completion
  transaction;
- due-tenant discovery, lease claim, database-clock deadlines, attempt
  accounting, stale-worker fencing, delivery settlement, retry, exhaustion, and
  ambiguous-failure recovery;
- global/per-tenant concurrency, permit-before-claim ordering, tenant fairness,
  and independence between delivery and key rewrap;
- Slack, PagerDuty, and generic-webhook delivery truth, including provider
  response semantics, bounded responses, redirect handling, and credential
  attachment after destination approval;
- cancellation, graceful shutdown, lease release/expiry, process restart, and
  worker supervision by the API server;
- active-key readiness and the tenant-scoped, cancellable, bounded rewrap pass,
  including discovery time in the pass budget.

The original TASK-007 packet, both remediation packets, both prior review
rounds and their validated findings, and the R2 verdict were used as context.
All earlier delivery-domain findings were reassessed against the cumulative
candidate rather than accepted from their prior closure statements.

## Authority and source coverage

The controlling sources read for this review were `AGENTS.md`,
`architecture/agent-rules.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-doctrine.mdx`, `architecture/wyrd-security-posture.md`,
`architecture/references/README.md`,
`architecture/references/architecture/patterns.md`,
`architecture/references/domain/analytical-operations-reliability.md`,
`architecture/references/languages/spec-driven-development.md`,
`architecture/references/languages/testing-workflows.md`, `TESTING.md`, approved
specification revision 35, the original task, and the R1/R2 remediation and
review artifacts.

| Obligation boundary | Sources traced | Result |
|---|---|---|
| Encrypted persistence and rotation (REQ-139, REQ-147, INV-006, INV-007) | `20260601000032_operator_connections.sql`; `operator_connections.rs`; operator connection service and key manager; `wyrd-crypt` envelope implementation; SQL/service tests | The database stores only ciphertext/envelope metadata, connection rows are tenant-isolated with forced RLS, secret replacement is atomic, rewrap is fenced by the observed secret version, and rewrap changes only the wrapped DEK/key version. |
| Transactional failed-run fanout and immutable context (REQ-097, REQ-138, INV-011, INV-013) | verifier-run completion SQL and Rust query path; dispatch migration/schema; verifier-run and delivery tests | A terminal failed verifier result freezes dispatch context and inserts one dispatch per active binding in the same transaction. Tenant-qualified foreign keys and the unique run/operator identity prevent cross-tenant linkage and duplicate logical fanout. |
| Claim, lease, database deadline, fencing, and attempt accounting (REQ-098, REQ-142, REQ-146, REQ-152, INV-015, AC-033) | `operator_dispatches.rs`; shared lease/settlement types; `verification/claims.rs`; SQL tests | Claims use `FOR UPDATE SKIP LOCKED`, database time, a fresh lease token, and status/token predicates on every settlement. Expired work is exhausted without a claim. Shutdown release refunds an unstarted attempt; ambiguous started work remains at-least-once and is recovered by lease expiry. Retry delays are clipped to the immutable delivery deadline. |
| Fair bounded concurrency and independent maintenance (REQ-098, REQ-146, AC-030) | claim loop; operator worker; runtime config; slow-endpoint, fairness, slow-rewrap, and stalled-discovery tests | Global permits are acquired before database claims; tenant limits are enforced; due-tenant discovery avoids a single global FIFO; delivery and rewrap are separately supervised capabilities, so maintenance latency does not consume delivery permits or hold back another tenant. |
| Attempt execution and provider truth (REQ-099, REQ-140, REQ-141, REQ-142, REQ-149, AC-029) | `verification/operators.rs`; Slack and PagerDuty modules; generic webhook redirect/body handling; provider tests | Each attempt resolves the immutable operator reference and the latest enabled connection, rechecks authority, opens the latest credential, and applies the remaining database-owned deadline. Generic delivery screens and pins every destination hop before attaching credentials. Slack application-level failure and PagerDuty HTTP outcomes are classified in their provider modules without contaminating shared retry machinery. |
| Cancellation, shutdown, and restart (REQ-115, REQ-146, AC-030) | `verification/mod.rs`; health capability registry; `app/server.rs`; claim loop; slow endpoint/restart/shutdown test | The API role starts and supervises scheduler, operator delivery, and rewrap capabilities. Capability crashes are observable and restarted. Shutdown stops new claims, gives in-flight work a bounded drain period, releases work not started, and otherwise relies on fenced settlement or lease expiry. |
| Boot readiness and bounded rewrap (REQ-147 and the design's key-rotation contract) | config validation; operator key manager; boot verification; cross-tenant version discovery; rewrap loop and tests | Configured active versions are validated as the exact persisted `i32`; production tenant key readiness is checked at boot; old keys are opened only for referenced versions; per-tenant work is transactional and cancellable; and the single absolute pass deadline now covers both cross-tenant discovery and tenant work. |

The execution path is reachable end to end: verifier completion creates durable
dispatches; the supervised claim loop acquires capacity and claims a leased row;
the worker resolves current connection state and credential material; provider
execution runs under the remaining database deadline; and a token-fenced SQL
transition records delivery, retry, release, or terminal failure. Rewrap runs
beside that path, discovers `(tenant_id, key_version)` pairs, and updates only
rows still at the observed version.

## Prior finding closure

The cumulative candidate closes the delivery-relevant R1 defects: unsupported
connection kinds cannot be registered; delivery and rewrap no longer share a
serial blocking loop; attempts use latest credentials and provider-correct
outcomes; shutdown/restart behavior is exercised; the generic redirect path is
revalidated before credentials are attached; and slow key work is isolated by
tenant and from delivery. It also closes the R2 defects: public key failures and
debug output do not expose selectors, active versions cannot be silently
truncated, Slack/PagerDuty classification is provider-local, and—most
importantly for R2 FIND-9—the cross-tenant discovery query is governed by the
same absolute deadline as the rest of the rewrap pass. I found no reachable
path that reintroduces those failures.

## Verification evidence

Fresh verification on the immutable candidate passed:

1. Six focused unit tests covering selector-safe key errors, redacted key
   debug output, exact key-version range validation, base64 key length, and
   Slack reply classification: **6/6 passed**.
2. Repository-managed Postgres setup plus migration/idempotence checks and the
   focused delivery tests
   `stalled_rewrap_discovery_returns_within_the_pass_budget`,
   `slow_rewrap_never_holds_back_another_tenants_delivery`, and
   `slow_endpoint_exhausts_the_budget_and_shutdown_releases`: **3/3 passed**.
3. Repository-managed Postgres setup plus
   `dispatch_delivery_obeys_budget_deadline_and_fencing`,
   `expired_dispatch_deadline_fails_without_a_claim`, and
   `updates_rotate_in_place_and_rewrap_is_fenced`: **3/3 passed**. Both Wyrd
   and Vala migration/idempotence setup checks also passed.
4. `git diff --check f8811ac5035c3aa165d34c38992f9889b3c9081f..57ee8dd0b3dddc3e1da34bf543ecc566bae2e831`:
   **passed**.

The task artifacts also record green broad verification for the owning SQL,
shared, and server lanes; journey lanes; code generation; boundary checks;
formatting; and lints. I treated those as available candidate evidence, not as
fresh executions performed by this reviewer.

## Verification limits

- I did not rerun the entire workspace gate or every broad lane; the fresh
  tests above target the persistence, fencing, timeout, shutdown, rewrap, and
  provider-classification risks in this review boundary.
- Live Slack and PagerDuty smoke tests remain external gated verification and
  were not run. Local protocol fakes exercise their request and response
  contracts without credentials.
- CodeGraph was unavailable because this checkout has no `.codegraph/`
  directory. Source and call-path review used the immutable diff and direct
  source inspection.

These limits do not prevent a delivery-domain verdict: the durable transitions,
worker call paths, and the high-risk concurrent/restart behaviors are covered by
direct inspection and focused executable evidence.

## Material findings

None.

## Overall result

**PASS** — the cumulative candidate satisfies the approved delivery,
persistence, concurrency, lease/fencing, retry/deadline, restart, and key-rewrap
obligations in this review boundary. No material correction is required.
