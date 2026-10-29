# TASK-001 remediation review — PASS

## Immutable subject and acceptance context

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Original base: `3899b0a35`.
Prior candidate: `826fcf9c742a36bcae5e8f68307e76685d88244e`.
Candidate: `5e7dec69732ba6d4d05399aeffd6f4d9c683745d`.
Requested remediation range: `e6a34a01092d44729456b17bb55553e08d63d322..5e7dec697`.

Reviewed `git diff 826fcf9c7 5e7dec697` with cumulative `3899b0a35...5e7dec697` available. The intermediate `826fcf9c7..e6a34a010` commit only adds R1 reports/task, so the requested range contains the same implementation patch. HEAD remained the recorded candidate; the initial working tree was clean. No source was changed, no prior report overwritten, and no commits, merges, pushes or deployments performed. No CodeGraph directory exists.

Authority and acceptance context: [approved specification revision 2](../../spec.md), [original TASK-001](../../tasks/TASK-001-object-inventory.md), [R1 verdict](../TASK-001-R1/verdict.md), [R1 independently validated findings](../TASK-001-R1/findings-validation.md), all selected R1 reports, and [TASK-001-R1 with implementation evidence](../TASK-001-R1/TASK-001-R1-inventory-lifecycle.md). Root and SQL-directory AGENTS, applicable agent guides, testing references and owning Card/Bifrost/security architecture remain unchanged.

## Round scope and independent review

This first remediation review checks five prior-finding closures, the incremental diff, and affected callers/consumers/invariants/tests. [scope.md](scope.md) records the common navigation map, reviewer selection and carry-forward rationale. Original source-backed acceptance for unchanged schema/catalog/layout/Blooms, shared upload validation, scoped reads/authorization/tenant/public-write boundary, Scribe/ACK isolation and other producers, removed SQL consumers, fitter/hosted/CLI and verification-runtime fixes carries forward. Cleanup concurrency is reopened because the remediation adds a durable failure commit before ACK waiting. No unrelated unchanged area was rediscovered.

| Fresh role | Selection and covered scope | Result |
|---|---|---|
| domain-concurrency-rev | Changed cleanup persistence/ACK ordering, failed/pending recovery, all sibling cleanup owners and concurrent claimed retries | [PASS](domain-review-concurrency.md), no material proposals |
| ponytail-rev | Independent all-five closure/source validation, incremental code/docs/tests, affected paths, carry-forward coverage, domain conclusion and mandatory gates | [PASS recommendation](findings-validation.md), explicitly empty remaining ledger |

Both selected reports are complete. A separate invariant, broad standards, maintainer, system or security team was unnecessary for this bounded remediation; their unaffected first-round evidence remains available. [claim-comparison.md](claim-comparison.md) records why no focused follow-up was needed: no material conflicting claim, unresolved failing path or untraced common cause remained. The validator discovered no new blocker requiring separate focused source validation. Agreement was not substituted for source proof.

## Prior-finding closure ledger

| Stable finding | Prior classification | Independently validated closure | Result |
|---|---|---|---|
| FIND-TASK-001-1 | INCORRECT | `hydrate_card` requires the returned envelope at the validated retained pointer before storage IO; missing/mismatched fact returns the existing visibility error. Existing pending/pointer/binding/identity/integrity behavior retained. New hydration proof and existing round trip pass. | CLOSED |
| FIND-TASK-001-2 | MISSING | Both initial reconciliation and abort compensation use `commit_card_failure`, committing Failed/cleanup pending before deletion ACK; only accepted deletion and successful physical cleanup clear cleanup. Existing claimed retry/explicit deletion retain ownership. New refusal/recovery/replay proof and cleanup regressions pass. | CLOSED |
| FIND-TASK-001-3 | VIOLATION | All enumerated changed-item docs and newly changed items inspected: Errors/Panics, fields, durable partial progress and retry boundaries covered. Closure is established from source, not lint success alone. | CLOSED |
| FIND-TASK-001-4 | VIOLATION | SQL absence proof explicitly requires DSNs; same migration/helper/assertions. Provisioned run passes; absent-DSN run fails at the required expectation with exit 100. | CLOSED |
| FIND-TASK-001-5 | VIOLATION | Changed upload-completion and download-init handlers have scrubbed handler instrumentation, excluding state/caller/headers/bodies as applicable. Focused route regressions pass. | CLOSED |

Exact locations, full producer/consumer and sibling tracing, documentation inventory and source evidence are preserved in [findings-validation.md](findings-validation.md). **Validated remaining finding ledger: empty.** No new FIND IDs, remediation task or specification revision is required.

## Reconciled acceptance matrix

| Obligation | Source and verification evidence | Result |
|---|---|---|
| REQ-002/003, INV-004: shared completion and preserved validation | R1 shared completion/validation evidence unchanged; focused Card round trip and recovery rerun | PASS |
| REQ-004: one inventory, attributed daily schema/Blooms, logical replay | R1 catalog/schema/fold evidence unchanged; explicit-deletion and compensation replay proofs pass | PASS |
| REQ-005/013, INV-001: envelope hydration requires inventory | Existing returned envelope is now required at the validated location before IO; missing-location refusal and restored hydration pass | PASS |
| REQ-005/013: file consumers and retryable readiness | Existing common file reader/consumers unchanged; readiness/tenant and normal round-trip reruns pass; invalid compensation producer state corrected | PASS |
| REQ-006/007/008, INV-003: existing delivery and isolated ACK | R1 source-backed delivery/producer acceptance carries forward; incremental outbox change is documentation only | PASS with original proof limits |
| REQ-015, INV-002: authorization/tenancy/public-write boundary | R1 scoped-read/security evidence carries forward; hydration retains binding checks and focused scoped-read regression passes | PASS |
| REQ-017: fresh-install authority and coordination | SQL removal unchanged; proof runs all assertions with Postgres and fails without setup | PASS |
| REQ-018: cleanup deletion and durable retry | Shared compensation recorder ordered after durable Failed/pending and before success; existing retry owner recovers refusal; deletion dominates stale completion | PASS |
| Mandatory changed-item rustdoc and handler tracing | Exact prior omissions and new items/source attributes independently inspected | PASS |
| Existing-owner reuse, non-goals and change scope | Existing hydration projection, failure/deletion lifecycle and test fault seams reused; no new API/dependency, inventory, queue, state, repair scan, hashing, retention, permission or deployment behavior | PASS |

## Reported cleanup overlap

Both reviewers independently traced the reported overlap: after A commits Failed/pending, B can claim cleanup while A waits; A's later unleased success can clear B's lease. A reaches that success only after completing physical cleanup and receiving the genuine deletion ACK. B repeats the completed work; absent-object deletion and terminal upload abort are idempotent, its terminal updates remain fenced, and the existing Card-wide deletion dominates completion replays. Failed cannot activate or become a new explicit-delete category. No required durable effect is missing in this interleaving, so it is not retained as a blocker or remediation request. Exactly one physical cleanup attempt or tombstone append is not an approved requirement. The overlap conclusion is source-derived, not claimed as a deterministically scheduled runtime test.

## Verification and limits

[verification.md](verification.md) records fresh review-time repository Postgres runs: Wyrd and Vala migration setup, both new tests, five affected existing Card regressions, and the exact SQL absence proof all pass. The same SQL selector without required DSNs fails as intended with nextest exit 100. Incremental whitespace checking is clean. The recorded linker compact-unwind warning did not prevent execution.

Remediation format/lints/codegen and its additional five Card regressions remain recorded implementation evidence; broader unchanged catalog/producer/outbox/fitter/CLI/hosted/boundary checks carry forward from R1 and the original task. Historical REDs were not reproduced in this review. Cancellation safety and concurrent success overlap are source-derived. R1's public controlled-ACK and queued-handoff experimental-versus-structural limits remain. No cloud/load/full language/deployment-backfill suites were run; integrated surface/performance closure remains at subsequent tasks/change review, and deployed migration/backfill is excluded by the approved spec.

This is the first remediation review; no repeated-unsuccessful-round convergence assessment applies. No required report or source is missing, every applicable obligation passes after independent closure reconciliation, and no unrelated implementation change entered the remediation.

**Verdict: PASS.** TASK-001's completion gate is satisfied at `5e7dec697`; all five prior findings are closed. This task review does not complete the entire multi-task change or invoke change completion.
