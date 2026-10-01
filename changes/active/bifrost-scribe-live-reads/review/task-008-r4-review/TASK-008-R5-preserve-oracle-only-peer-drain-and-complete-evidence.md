---
id: TASK-008-R5
title: Preserve Oracle-only peer drain and complete R4 documentation and evidence
kind: remediation
status: ready
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 20
requirements: [REQ-004, REQ-005, REQ-014, REQ-015]
acceptance: [AC-016, AC-017]
parent_task: TASK-008
remediates: [FIND-007-10, FIND-007-11, FIND-007-12]
---

# TASK-008-R5

Route to `$wyrd-implement`. Correct the remaining Oracle-only preservation failure and two explicit completion-rule violations. Preserve the functioning blocked-Scribe fix and closed R4 findings.

## Inputs and authority

Approved spec: `changes/active/bifrost-scribe-live-reads/spec.md`, revision 20. Original tasks: `tasks/TASK-007-one-parquet-scan-for-live-reads.md` and `tasks/TASK-008-tenant-proven-per-file.md` under this packet. Prior remediation: `review/task-008-r3-review/TASK-008-R4-preserve-remote-refusals-and-close-peer-shutdown.md`. Current independent evidence: sibling [verdict.md](verdict.md), [findings-validation.md](findings-validation.md), [followup-review.md](followup-review.md).

Candidate: `ca99db0af5a0d898ef67834699405c1c73719f56`; correction parent: `2f188cb6185061a43db36122aad68b5e253308d1`; cumulative TASK-007 base: `a7582db587c6170a290760f1741673125612b797`; TASK-008 base: `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`.

Current authorization is STATIC ONLY: no cargo, nextest, mise, builds, tests, benchmarks or commits. This task grants no execution exception. Future runtime proof below requires separate authorization. Do not invent results to close evidence.

FIND-007-7/8/9 are closed. FIND-007-3 stands; Postgres tenant columns are excluded; error code `WYRD_VALA_500_QUERY_TENANT_INVARIANT` and no live-read cap remain unchanged. Complete capacity qualification is outside this remediation.

## Diagnosis and selected correction

### FIND-007-10: incomplete Oracle-only lifecycle preservation

R4 correctly fixes the blocked-Scribe response at accepted IO: an owned cancellation future fails socket read/write/flush even when HTTP/2 capacity prevents response-body polling. Source refs, fragment streams and staged holds can then drop. Keep that behavior for Scribe-containing fragment listeners.

The incomplete constraint is the expressly preserved Oracle-only peer/Analytical lifecycle. `app/server.rs:678` unconditionally chooses the new runner for all private listeners. `grpc/mod.rs:405–478` constructs one for query-only pods with Analytical/lifecycle/forward-query services but no Scribe fragments. During shutdown, `state.rs:1927–1938` removes readiness/new admission; `start_draining:640–645` does not cancel existing Oracle work. Supervision cancels transport next (`app/supervise.rs:184–202`), and StoppingIo immediately fails accepted Oracle-only socket IO. Role/engine cancellation occurs later (`state.rs:716–725,1988–1995`). Previously the existing runner allowed accepted streams to finish during tonic's bounded graceful transport interval.

Consequently an admitted exchange/forwarding response that could complete during that interval now receives premature transport failure. Analytical lease drop may begin Cancelled settlement. The existing cleanup driver remains correct: this is loss of required connection semantics, not demonstrated tenant leakage, acknowledged-data loss or suppressed cleanup. Cleanup after the reset does not satisfy preservation of the previous graceful interval.

Selected correction: use the existing server role/capability composition at the private listener owner to choose cancellation-aware accepted IO only for listeners serving Scribe fragments, including mixed-role listeners. Keep Oracle-only listeners on the existing `serve_grpc_with_listener`. Reuse both existing runners; the unintended reset is produced by listener selection, so fix it there. Do not add downstream Analytical guards or change lease cleanup, delay every server shutdown, introduce a pump/timeout/framework, or grant indefinitely blocked Oracle work unlimited drain. Existing process deadline, later role cancellation, mTLS, public listener and durable settlement remain authoritative.

### FIND-007-11: required item documentation missing

AGENTS §16 and agent-rules require documentation for each changed item, including private associated types, and Errors sections for every fallible method. New `StoppingIo` methods at `wyrd-tonic/src/server/mod.rs:321,334,345,361,368` return fallible Poll values but only have summaries. Its ConnectInfo at :374 is undocumented. Enclosing type documentation does not replace the explicit per-item requirement.

Selected correction: document actual cancellation and forwarded socket errors for the five methods and the exact TCP connect-info type. Read/write/flush refuse cancelled IO; write-half shutdown remains delegated after cancellation. ConnectInfo remains the identity tonic wraps with TLS certificates. Preserve implementation behavior; no new test, abstraction or check is required for this documentation correction.

### FIND-007-12: focused command evidence incomplete

The R4 evidence at `review/task-008-r3-review/TASK-008-R4-preserve-remote-refusals-and-close-peer-shutdown.md:92–104` names classifier and remote staged-footer tests with RED/GREEN results. It records only focused commands for the blocked-window and lost-Scribe cases. AGENTS §11 and spec-driven-development require exact focused commands for every named test. The broader green Oracle lane remains credible behavior evidence, but does not replace this record.

Selected correction: append the actual exact classifier and remote-footer commands and selected-test results with attribution. If those focused runs did not occur, execute them only after separate authorization and record observed results. Source/target names are known; do not alter tests, invent historical RED/GREEN, rerun capacity, or broaden gates. Reuse repository-pinned nextest and the environment-owning Postgres/migration recipe.

## Acceptance and focused proof

| Finding | Acceptance |
|---|---|
| FIND-007-10 | Oracle-only private listeners retain the existing graceful transport runner; Scribe-containing listeners retain stopping IO. A real authenticated Oracle-only admitted operation that can complete during ordinary drain preserves its existing completion/lifecycle result. Blocked Scribe stop still completes before resuming reads and releases producer/Oracle holds. Process deadlines, later role cancellation and durable cleanup remain unchanged. |
| FIND-007-11 | All five IO methods document their real Errors behavior; ConnectInfo documents the forwarded identity. No executable changes from this obligation. |
| FIND-007-12 | Exact focused commands match actual source names/targets, include required setup and record selected-test results with honest attribution. |

Runtime change uses ordered RED/GREEN/REFACTOR once execution is authorized: first prove the Oracle-only admitted-operation regression using the existing real peer harness; make the role-boundary correction; retain blocked-Scribe, pending-source, mTLS/public and Analytical loss-settlement behavior. No new harness or synthetic load. Implementer owns the new test name and must record/run its exact selector.

The following are FUTURE recipes, not executed review proof. Existing manifests own test-support features. For the two missing R4 records, preserve actual commands if available; these source-confirmed recipes describe the intended focused checks:

```sh
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::dispatcher::tests::open_stream_status_preserves_tenant_refusal)'

scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::remote_staged_footer_refusal_fails_closed)"'

scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=peer_network::analytical::remote_live_window_blocked_scribe_stops_cleanly) | test(=peer_network::analytical::remote_live_scribe_drop_releases_query)"'
```

Broader future verification: `mise run fmt`, `mise run lints`, `mise run docs:check`, `mise run test:bifrost:journey:oracle`, `mise run test:server:peer`. Recheck whitespace against original cumulative base and this candidate. Read tracing for any runtime failure before diagnosing it. No contract generation change is selected; no codegen, capacity rerun or broad gate is needed unless implementation actually changes the relevant scope.

## Preserved scope and handoff

Preserve streamed Aborted/TenantInvariant, first-refusal canonical audit, exact mTLS connect info, cap-free shallow references, normal pull backpressure, shared query pool and source/lease ownership, native/wire terminal validation, absolute data roots, v6 digest/proto reservations, ACK/WAL/publication fences and role settlement. Do not refactor unrelated transport, change public or durable contracts, or reopen fixed maintainer decisions.

A later independent review reassesses the complete cumulative candidate. This task authorizes no merge, push, deploy or commit and grants no runtime execution exception.

## Implementation evidence (2026-09-30)

Execution was authorized by the maintainer ("continue with recommendations and fixes").

| Finding | Implementation | Verification | Result |
|---|---|---|---|
| FIND-007-10 | `app/server.rs` peer task picks `serve_peer_grpc_with_listener` only when `bifrost.scribe().is_some()`; Oracle-only listeners run the unchanged `serve_grpc_with_listener` | blocked-window, lost-Scribe and footer journeys pass; `test:server:peer` 11/11 | PASS |
| FIND-007-11 | `# Errors` on the five `StoppingIo` IO methods; `ConnectInfo` documented | `mise run lints` exit 0 | PASS |
| FIND-007-12 | exact commands below | results below | PASS |

No new Oracle-only drain journey was added: the Oracle-only listener now runs the same graceful runner it ran before R4, so the regression is removed by construction rather than guarded downstream.

Focused commands, all exit 0:

```sh
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::dispatcher::tests::open_stream_status_preserves_tenant_refusal)'
# 1 passed

scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all \
  -E 'test(=distributed::remote_staged_footer_refusal_fails_closed) | test(=peer_network::analytical::remote_live_window_blocked_scribe_stops_cleanly) | test(=peer_network::analytical::remote_live_scribe_drop_releases_query)'
# 3 passed: footer 2.7s, lost-Scribe 12.3s, window-blocked 17.5s
```

Also: `mise run fmt`, `mise run lints`, `mise run test:server:peer` (11/11).

R4 RED records (before the fixes): classifier test failed with Aborted mapped to Unavailable; footer journey `rows=0 Degraded None`; window-blocked journey `Bifrost shutdown deadline elapsed before role drain`.
