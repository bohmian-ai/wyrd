# TASK-003 independent invariant review

**Result: FAIL — two residual acceptance/evidence findings proposed for independent validation.** Neither finding establishes a production duplication or admission defect.

## Immutable subject and method

Base `ef3a57b0776dd7bc451273b9176498fcca700550`; candidate `ea317179820b24d8b08ed499454ce2cf4675ce4e`. HEAD was checked and remains candidate. Reviewed the cumulative range, using `9ffdf753a...ea3171798` only as the remediation locator. The current TASK-003 specification text is owner-authorized; the revision-14 pre-release schema addition is excluded, as instructed. The original task, R1 remediation, prior verdict/validation/reassessment and supplied diagnostic reports were read as claims to check. No other current discovery report was read.

Authority applied: supplied AGENTS.md, agent rules, task-review/implementation skills, spec-driven-development and maintainer-style, testing workflows, REQ-005/006 and INV-001–005, applicable Wyrd verification/audit design, Bifrost ACK boundary, security posture and reliability/recovery. CodeGraph was used before code location/reading. The scope includes runtime admission, server direct/enqueue/runner/claims/Operator siblings, both sinks, HTTP/MCP error projection, SDK pressure stories, changed test-boundary projections and helper callers, operational prose and evidence. Manifests retain existing Tokio/runtime and contract ownership; no new dependency or Cargo feature was added.

## Values, ownership and lifecycle

- `Outbox::reserve` charges only `Admission.reserved`; `stage` charges only `Admission.unreserved`. The shares sum to finite capacity. Sending increments `pending` and `staged_reserved` before queue publication under the queue read lock. Dequeue, dispatch and retries retain the charge. A successful write or terminal task loss releases precisely the item count and its reserved subset. The failed tenant slice remains separate from later arrivals, preserving batch identity.
- `Reservation` spends each pre-admitted slot once; its drop releases only the unused remainder. Direct execution reserves two Scribe items before the engine and stages the inputs/result pair only after completion. Enqueue validates/builds its submission and row, then reserves the downstream SubmissionOutbox and Scribe slot before source staging. Failure of the second reservation drops the first without a source row.
- Scribe retains the downstream reservation inside `ExecutionInputs` across queueing, retries and frames. Only non-lost rows hand off after successful Scribe completion. SubmissionSink calls existing `VerifierExecutionQueue::accept_submissions` through a TenantConn and commits once per tenant slice. Replay remains the SQL owner's existing responsibility; no second idempotency owner or producer SQL was introduced.
- The runner reserves its result slot before dispatch; pressure becomes `Transition::Defer`, preserving the existing lease-fenced settlement/attempt refund path. Non-completion drops the reservation. The shared ClaimLoop caps its JoinSet before each claim; Operator retains its existing permit owner and receives its existing global limit as the active cap.
- Audit, gateway capture and span producers still use unreserved staging. Overflow logs/counts loss without a result error or persistence wait in permission evaluation. The span permit cap remains an earlier producer bound, not a second outbox writer.
- Shutdown closes the sender under the write lock and waits for the writer to disappear before final accounting. A nonzero `pending.swap(0)` now selects the sole owner of both pending and staged-reserved loss. A concurrent loser returns before touching staged-reserved; unspent reservations remain separately owned. This closes the old split-settlement race. Late sends release their own admission and do not mutate pending/staged-reserved when the queue is absent.

Existing owners are sufficient throughout: Outbox/OutboxWriter, VerificationControl, ClaimLoop, ScribeSink, SubmissionSink and VerifierExecutionQueue. The added Admission/Reservation state expresses the finite bound and ACK handoff within those owners. No replacement transport, durable outbox, distributed allocator, tenant SQL filter or framework was needed or found.

## Acceptance matrix

Verification described as “recorded” is implementation evidence, not independently rerun here.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Whole pending process work bounded; queued/waiting/retry/in-flight retain charges | `outbox.rs:21–34,227–300,429–474,666–681`; both sink capacities | Independently executed exact seven-selector outbox run: 7 passed | PASS |
| Direct/refused calls cannot stage half a source pair; typed retry timing crosses HTTP/SDK/MCP | `service.rs:485–489,671–685,726–743`; `error.rs` catalog; HTTP Retry-After mapper; existing shared SDK projection | Source inspection; recorded three SDK pressure passes, MCP negative flow and mapper/codegen results | PASS |
| Scribe ACK→Submission handoff finite, retry identity and existing durability preserved | `scribe_outbox.rs:128–135,903–962`; `submissions.rs:27–60,90–119`; queue acceptance `verifier_executions.rs:1078–1160` | Source trace; independent identical-slice retry test; recorded real-server proof | PASS |
| Ordinary active queued work bounded at claim owner | `claims.rs:154–180,235–281`; `runner.rs:234,361–387`; Operator cap remains its existing global limit | Recorded first-round cap test; current source limits spawn count before claims | PASS |
| Audit remains non-blocking, counted and independent of authorization | `outbox.rs:227–234`; unchanged audit callers; `scribe_outbox.rs:623–635` | Independent bounded-audit test passes; security/recovery prose consistent | PASS |
| B completes while A continues finite offers, then accepted work drains and source rows reconcile | Rust pressure story `verify_in_real_time.rs:1237–1384`; Python `test_verify_in_real_time.py:480–521`; TS `verify-in-real-time.test.ts:454–522` | All three loop normal A submits until B's wait finishes, retain held-A assertion, overload/refusal and `3 + offered` reconciliation; recorded 1 selected pass each | PASS |
| Exact-key pressure→drain→retry→accepted replay establishes durable final execution cardinality | HTTP test `pg_verification_routes.rs:496–542`; existing replay test `:381–458` | New test reconciles refused source and observes durable recovered row, but final replay count has no downstream completion barrier; INV-R2-001 | FAIL |
| Concurrent shutdown releases reserved/unreserved loss exactly once, retaining unspent reservation | `outbox.rs:336–367`; regression `:1251–1292` | Independent exact regression + ordinary shutdown drain/loss tests pass; source single-owner proof covers interleaving | PASS |
| Touched Rust Errors/Panics documents match changed behavior | Execute/enqueue Errors, Scribe constructors, outbox tests, mapper test, retained reader and new HTTP helper/test | Source inspection; recorded fmt/lints | PASS |
| Normative audit/operations promises match bounded overflow loss | `wyrd-security-posture.md:466–482`; `operations/reliability-and-recovery.md:71–83`; Wyrd design submission paragraph | Static producer/sink contract comparison; no runtime test required for prose | PASS |
| Independent diagnosis evidence is inspectable for original and new failure-driven adaptations | Reports A/B plus supplemental C/D/E; R1 evidence row at `TASK-003-R1-close-admission-review.md:151` | Original five diagnoses covered with explicit retrospective limits. Two new HTTP failure-driven test edits have only implementer diagnosis; INV-R2-002 | FAIL |
| Tenant/authority isolation, no producer Postgres, one Scribe writer, portable facilities; no new durable receipts/quota/fairness claims | TenantConn unchanged in sink; shared native projections are test capacity knobs; full cumulative source diff | Static diff/caller inspection; no SQL/dependency changes | PASS |
| Standalone work independent of grouping; no TASK-001/004 behavior imported | Ordinary verify/submit stories and existing input schema; task evidence keeps grouped J-005 open | Grouped integration/benchmarks not run, expressly assigned to integration/change review | PASS for task-local obligation |

## Prior-finding closure

| Stable ID | Disposition | Current source/evidence |
|---|---|---|
| FIND-TASK-003-1 | CLOSED | Only the nonzero pending-swap winner settles staged-reserved; current-thread joined shutdown regression proves one loss/count and exact remaining unspent slot. Seven selected tests pass. |
| FIND-TASK-003-2 | CLOSED | Each SDK story actually issues A normal submits while the B wait is outstanding, stopping offers only after B completion is observed; accepted-source totals include every offer. No strict fairness inference. |
| FIND-TASK-003-3 | PARTIALLY CLOSED; retain stable ID | Fixed body/key/authority now survives refusal/drain/retry; recovered execution and source cardinality are checked. Final accepted replay's asynchronous completion is not synchronized before cardinality; INV-R2-001. |
| FIND-TASK-003-4 | CLOSED | Previously identified item error/panic contracts and newly introduced/widened Rust items have matching docs. |
| FIND-TASK-003-5 | CLOSED | Security and reliability owners now state finite capacity and counted/logged non-blocking loss. Identical admitted retries, one writer and accepted crash-loss boundaries remain. |
| FIND-TASK-003-6 | Original subcases CLOSED; residual same-cause evidence gap | C recovers admission diagnosis; D/E explicitly retrospective and bounded; A/B remain intact. R1 added two unexpected HTTP-test fixes without corresponding supplied independent diagnosis, INV-R2-002. |

## Proposed findings

### INV-R2-001 — FIND-TASK-003-3: final replay cardinality is observed before the replay must reach its durable sink

**Classification:** MISSING acceptance proof. **Obligation:** R1-AC-3 and the retained FIND-3 correction require accepted replay plus one durable execution after exact-tuple refusal/recovery.

**Location:** `crates/wyrd/wyrd-server/tests/integration/pg_verification_routes.rs:532–541`.

**Evidence:** The test performs `input_rows` before replay, which flushes only the earlier source writes. It then calls enqueue, compares the returned deterministic execution ID, immediately reads `fixture.queued()`, and shuts down. Production enqueue returns immediately after staging (`service.rs:674–685`); ScribeSink submits to SubmissionOutbox only after ACK (`scribe_outbox.rs:950–962`); SubmissionSink then independently awaits its tenant transaction and commit (`submissions.rs:95–101`). `fixture.queued()` is a point-in-time SQL read, not a writer drain. Thus a reachable schedule executes the final count before the replay is even handed to SubmissionOutbox. Shutdown later drains it, but no post-drain assertion follows. `flush_bifrost` itself only settles the ScribeOutbox and flushes Scribe (`wyrd-testing/src/server.rs:1231–1253`), so adding it alone would not prove the submission commit.

**Consequence and limits:** The new composed proof can pass while its final replay remains unprocessed. Existing queue source is idempotent (`verifier_executions.rs:1100–1125`) and the older replay/conflict test exercises that owner; no actual duplicate execution is established. The remaining issue is exactly the durable composed acceptance evidence required by the previous finding, rather than a new replay policy.

**Smallest testable correction:** Extend this existing HTTP test to observe downstream completion after the replay before checking final durable cardinality. Reuse an existing completion/drain boundary if available, or the older replay test's ordered same-tenant sentinel pattern: enqueue a distinct later request, wait for its durable execution through `visible_execution`, then check the total including that known sentinel. The per-tenant ordered Scribe and submission slices put replay before that later accepted write. Preserve source-absence proof for the refused attempt and do not require accepted-replay source-row deduplication, alter production identity or add a new harness/transport. Run only the two exact HTTP selectors already assigned by R1.

### INV-R2-002 — FIND-TASK-003-6: new failure-driven HTTP test adaptations again lack independent diagnosis reports

**Classification:** VIOLATION of required evidence. **Obligation:** task-review failure-diagnosis rule; `.agents/skills/wyrd-implement/SKILL.md` “Diagnose unexpected failures”; R1's explicit diagnostic execution discipline.

**Location:** R1 remediation evidence `changes/active/long-horizon-verification/review/task-003-9ffdf753a/TASK-003-R1-close-admission-review.md:151`, and changed HTTP test/helper `pg_verification_routes.rs:466–480,516–541`.

**Evidence:** The evidence expressly records two unexpected red runs and edits: move queue cardinality after drain because executions appear only after Scribe ACK, and remove `CAST(... AS VARCHAR)` because the retained text reader rejects the resulting view type. These touch test assertions/query logic; the first is also an ordering cause. The implementation skill requires a fresh read-only diagnostician before such fixes and its report beside the diagnosis. Supplied A/B and C/D/E cover the five original implementation diagnoses only. None covers these R1 failures or supplies their failing trace/compiler output for independent cause inspection. The implementer's explanation and later two-test Green do not substitute for the mandated report. This is the same evidence-production cause as FIND-6, not a new stable ID per test edit.

**Consequence and limits:** The review cannot independently check whether these failure-driven adaptations preserve their intended observation boundary from the supplied failure evidence. Source supports both explanations; no faulty cast removal, weakened tenancy or erroneous production change is inferred. Original diagnoses 1–5 remain closed.

**Smallest testable correction:** Supply recovered independent reports if they exist. Otherwise preserve the actual failing commands/output where available and obtain one fresh read-only retrospective diagnosis of both related HTTP test failures, clearly stating historical limits, cause, shared owner and affected callers. Reuse existing retained-record decoding and staged-write lifecycle owners. No production change, full suite, baseline checkout or old-code rerun is needed solely for this evidence gap. Store new evidence only in the currently authorized report directory, leaving original task/prior directory immutable.

## Independent execution and limits

Executed once, without additional features or broad lanes:

```sh
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::concurrent_shutdown_releases_reserved_losses_once) | test(=outbox::tests::bounded_staging_covers_retrying_and_inflight_work) | test(=outbox::tests::bounded_audit_staging_remains_nonblocking) | test(=outbox::tests::a_retry_writes_the_identical_failed_slice_without_later_items) | test(=outbox::tests::a_failing_tenant_does_not_delay_another_tenant) | test(=outbox::tests::shutdown_flushes_items_that_recover_before_the_deadline) | test(=outbox::tests::shutdown_counts_items_unwritten_at_the_deadline_as_lost)'
```

Exit 0. Nextest run `d7e8d8af-3e40-4461-b703-ce828a8fecca`: **7 run, 7 passed, 62 skipped**, one binary, default profile. No failure required diagnostic rerun. The concurrent-shutdown test does not deterministically force the old between-swaps interleaving; the current source-level single-winner argument supplies that closure.

No SDK/HTTP/MCP runtime test, formatting/lint aggregate, grouped journey, load benchmark or broad suite was independently run by this reviewer. Their recorded outcomes were checked against selectors/source but remain recorded evidence. Source/tests/task/spec/prior review were not edited; this report is the only write.
