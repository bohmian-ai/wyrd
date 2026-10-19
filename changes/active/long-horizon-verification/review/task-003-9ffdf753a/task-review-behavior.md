# TASK-003 behavior review

Overall result: **BLOCKED** on specification authority. Under the unchanged TASK-003 obligations, the proposed findings below would require correction before acceptance.

## Immutable subject and authority

- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Candidate: `9ffdf753ab11ce9b638c906388d3690a6aede249`.
- Reviewed the complete `git diff ef3a57b07...9ffdf753a`, including all 32 changed files, and expanded the starting navigation map through the surrounding owners and sibling consumers.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-003-bounded-outbox-admission.md`.
- Requested approved specification: revision 13. The supplied `spec.md` is revision 14/draft and records revision 13's approval in its history. No exact approved revision 13 artifact or owner clarification was supplied. I have not approved revision 14. The findings concern unchanged REQ-006, AC-005/006 and task obligations, rather than the added table-schema text.
- Governing material inspected: repository AGENTS instructions, `architecture/agent-rules.md`, spec-driven-development and maintainer-style references, reference router, testing/error/reliability references, Wyrd doctrine, and applicable Wyrd/Bifrost architecture sections for verification submission, outbox audit, tenancy, admission and loss boundaries.

## Expanded caller and reuse map

| Path | Inspected owner and result |
|---|---|
| SDK direct call → HTTP execute / MCP execute | Both server adapters use `VerificationControl::execute`. It authorizes, reserves two Scribe slots before execution, and stages inputs plus result through the reservation. No second client admission policy was introduced. |
| SDK submit → HTTP enqueue / MCP enqueue | Both adapters use `VerificationControl::enqueue`. It builds bounded declarations and reserves a SubmissionOutbox slot followed by a Scribe slot before staging the input. Failure of either reservation stages no source row; RAII releases earlier unused reservations. |
| Scribe ACK → durable execution queue | `ScribeWrite::ExecutionInputs` carries `Reservation<SubmissionSink>`; `ScribeSink::write` hands off only after the complete slice succeeds, excluding lost inputs. `SubmissionSink::write` reuses `VerifierExecutionQueue::accept_submissions` in its tenant transaction. |
| Durable claim → engine → result | `VerifierRunner` uses the existing `ClaimLoop`; active tasks stop at `RuntimeLimits::execution_slots`. Result capacity is reserved before the engine runs; pressure becomes `Transition::Defer`, preserving the attempt-refund mechanism. Settlement consumes the reservation only when the fenced completion applies. |
| Shared queue siblings | AuditStage, gateway captures and tenant span copies remain unreserved Scribe writers; OperatorWorker uses the same ClaimLoop and retains its existing process/per-tenant permits. SubmissionOutbox is the existing second stage, not a newly introduced queue. |
| Retry/loss/release | Generic writer waiting, failed and in-flight owners carry the reserved count. A failed tenant retains the identical slice and admission; success or terminal loss releases it. Shutdown contains the new accounting regression described in B-001. |
| Existing alternatives | Generic Outbox already owns buffering, tenant dispatch, retries and shutdown, so extending it is appropriate. `VerifierExecutionQueue` already owns replay/conflict and tenant claim ordering; no replacement identity resolver or scheduler was needed. The client `RecordQueue` has byte-budgeted buffered Bifrost ingestion, but owns client batch transport rather than server-internal Scribe writes and cannot replace this workflow without violating the single server outbox boundary. |

## Acceptance matrix

PASS here means the source satisfies that bounded obligation with the listed evidence; it does not resolve the specification-authority blocker. Recorded test results are claims unless listed separately as checks actually run.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Whole pending work has finite item accounting, including queued, waiting, retrying and in-flight work | `outbox.rs` Admission, Reservation, writer backlog/in-flight accounting; shares sum to capacity | Two new focused outbox tests independently run, 2 passed | PASS |
| Retries retain identical slices and do not double-charge | Writer `failed` backlog retains items/count; admission occurs only at producer boundary | Existing identical-slice regression inspected; task records its successful execution | PASS |
| Slots return on normal write, loss and unused reservation drop | Writer release, send failure release and Reservation::drop | New bound test independently passed; shutdown caveat B-001 below | PASS for normal paths |
| Preserve existing shutdown/drain/loss behavior | New shutdown cleanup splits ownership across two swaps | Existing deadline tests are single-shutdown tests; no concurrent reservation cleanup proof | FAIL — B-001 |
| ACK → SubmissionOutbox cannot be an unbounded hidden handoff or discard an admitted submission for capacity | Reservation<SubmissionSink> is retained in ExecutionInputs through ACK, then spent by the existing Scribe writer | Source trace plus generic reservation test; no independently run ACK-saturation journey | PASS, verification limit recorded |
| Bound claimed execution tasks, rather than only hosted step concurrency | ClaimLoop checks active count before and within each round; VerifierRunner supplies execution_slots | New 3-tenant/2-slot integration assertion inspected; recorded 51 runtime/route tests passed | PASS |
| Refuse producer overload before staging any source row | Direct reserves two items; enqueue reserves both existing stages before `.stage` | All three SDK journeys and MCP pressure test inspect typed refusal and no extra inputs; recorded passes | PASS |
| Typed retryable SDK/HTTP/MCP pressure with timing | Derive-backed VerificationStagingFull, retry detail, HTTP Retry-After mapper; shared control used by MCP | SDK/MCP new tests and HTTP mapper assertion inspected; codegen claimed passed | PASS |
| No partial direct two-row staging caused by pressure | Both direct slots reserved as one admission before executing | Two-row reservation source; recorded end-to-end refusal/readback | PASS |
| Audit remains non-blocking, counted and unable to fail authorization | Unreserved stage returns no error; overflow logs and increments loss; authorization remains before reserve | New audit-bound unit test independently passed; task records setup audit drops without operation failure | PASS |
| Tenant B completes while A continues offering excess calls | New journeys complete B after only the first A direct call, before filling the bound; A makes no further call while B waits | Required continuing-pressure scenario is absent; existing failing-sink unit test does not cover it | FAIL — B-002 |
| Retry the same refused enqueue identity after drain and preserve no-duplicate replay | Server identity derivation/queue replay are retained; each new public SDK submit call generates a new key | New journeys repeat a Run call, not a key; existing HTTP replay test has no preceding staging pressure | FAIL — B-002 |
| Accepted work drains and ordinary runtime recovery remains available | Existing outbox and claim lifecycle retained; added cap does not replace durable leases/fences | Single-shutdown regressions and existing runner crash/restart tests inspected; recorded runtime pass, not independently rerun | PASS subject to B-001 |
| Standalone proof independent of group_id; grouped J-005 evidence deferred until integration | No TASK-001 dependency or grouped producer changes | Standalone journeys exist; task explicitly marks grouped rerun open | PASS for task scope; integrated evidence open |
| Single Scribe path; no durable outbox, distributed quota, strict share/fairness, platform-specific limiter or producer Postgres access | Existing Rust owners extended; enqueue remains synchronous/no SQL; no dependency or SQL files changed | Full write-set inspection | PASS |
| SDKs remain projections; errors and testing knobs have declaration parity | Rust catalog; Python boundary/stub and TS native source/generated declaration changes agree | Recorded Python/TS lint/type/declaration/codegen checks | PASS |
| No stronger durable receipt or crash-loss claim | Submission still responds after memory staging; ACK handoff and existing loss boundary retained; architecture updated | Source and documentation inspection | PASS |

## Proposed findings

### B-001 — REGRESSION: concurrent shutdown can split lost-count and reserved-count ownership

**Obligation:** TASK-003 preserves existing shutdown/loss accounting, releases capacity on terminal shutdown and does not regress the shared Outbox contract. `Outbox::shutdown` explicitly promises that concurrent callers are supported and exactly one reports losses.

**Location:** `crates/shared/wyrd-runtime/src/outbox.rs:348–350`, consuming `Admission::release` at lines 438–441.

**Producer-to-consumer evidence:** A reserved staged item increments `pending` and `staged_reserved` in `send`. If a sink remains blocked through the shutdown deadline, the writer exits with both counts still nonzero. Two callers can concurrently pass `writer.wait()`. Caller A swaps `pending` to zero and is preempted; caller B reads `lost = 0`, swaps the nonzero `staged_reserved` count to zero, and calls `release(0, reserved)`. `count - reserved` then underflows. A has the opposite mismatched pair and also releases the wrong share. The new reserved-count swap introduced this race; the original single pending swap gave one caller ownership of all cleanup.

**Reachability and sibling trace:** The public runtime handle is shared as `Arc<Outbox<_>>`, and shutdown explicitly permits concurrent calls. ScribeOutbox and SubmissionOutbox both inherit this method. Production and test lifecycle owners normally issue a single shutdown; this finding is about preserving the existing shared method contract, not claiming those normal owners currently race. Reservations make the failing state reachable for ordinary accepted verification writes. Writer finish releases both counts together before `writer.wait()` returns and cannot repair counts left after deadline abandonment.

**Observable consequence:** A permitted shutdown race can panic in checked builds and corrupt admission counters in unchecked arithmetic builds; the loss-reporting contract and reserved/unreserved accounting cease to hold during shutdown.

**Smallest correction:** Keep cleanup in the existing Outbox owner. Make the caller that wins the nonzero pending swap own the associated reserved swap and release; a zero-loss caller must not consume the other counter. No new lifecycle owner, queue or capacity service is needed. Preserve unused-reservation drop behavior and the existing single-shutdown deadline accounting.

**Focused closure proof:** Exercise concurrent shutdown with reserved staged work abandoned at a deadline; both callers complete, exactly one reports the lost total, losses are counted once, and pending plus both admission shares return to their correct values. Existing normal drain and deadline-loss tests remain green. This proposal is source-proven interleaving analysis; no race reproduction was executed during this review.

### B-002 — MISSING: the required continued-burst and same-identity recovery proof is not present

**Obligation:** TASK-003 Scenario 3 expressly requires accepted B work to finish while A continues offering calls, plus retry of a refused enqueue after drain without duplicate execution. AC-005/J-005 specifies the same enqueue identity. These are observable acceptance requirements, not an optional performance benchmark.

**Locations:** `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:1228–1259,1295–1308`; mirrored sequences in Python `test_verify_in_real_time.py:478–513` and TypeScript `verify-in-real-time.test.ts:451–498`. The producer of retry identities is the unchanged `crates/shared/wyrd-client/src/verify/mod.rs:444`.

**Evidence:** The test starts A's first direct judgment, submits B, then waits for B to finish. It starts A's second call only after that wait. Thus B is not tested while A continues offering calls or after the pressure condition has been established. Later, after two A calls occupy capacity, verify and submit are refused and the A calls drain. The recovery invocation reuses `refused_run`, but `Target::enqueue` generates `verification-submit-{Uuid::now_v7()}` on every invocation, so it receives a fresh idempotency key. The final three-row count proves refusal plus a new successful call; it cannot prove recovery/replay of the refused enqueue identity. Python and TypeScript project this same shared client producer.

**Existing alternatives inspected:** The HTTP `enqueued_executions_replay_conflict_and_read_back` integration test already accepts an explicit stable key and checks replay/conflict through the real server, but does not fill staging before retry. The generic failing-tenant outbox test proves sink-retry independence, not continued producer pressure through public calls. The new execution-slot integration test proves a cap, not the required continuing-traffic outcome. The task's broad recorded green results therefore do not close these specific missing scenarios.

**Observable consequence:** The acceptance evidence can remain green if progress fails specifically during continuing offered overload, or if pressure recovery/replay does not preserve one logical enqueue. This is an evidence gap, not an assertion that the current production identity implementation has a demonstrated duplication bug.

**Smallest correction:** Extend the existing journey story with controlled continuing A requests while accepted B work completes and observe accepted/refused responses plus drain. For exact-key recovery, reuse the existing HTTP enqueue/replay owner and stable key support: refuse that key under capacity pressure, retry it after drain, replay the accepted identity and prove one durable execution. Do not add a public idempotency API, replacement load harness, benchmark, host stress or fairness guarantee merely to repair this proof. Existing SDK pressure projection assertions remain useful.

**Focused closure proof:** Run the affected exact journey selectors and the focused HTTP recovery/replay selector, showing B completion before A stops offering requests, no rows from refused calls, bounded accepted work/drain, and one execution for the stable recovered key.

## Checks actually run and limits

Executed:

```sh
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::bounded_staging_covers_retrying_and_inflight_work) | test(=outbox::tests::bounded_audit_staging_remains_nonblocking)'
```

Result: **2 passed, 66 skipped**, nonzero intended selection, 0.357 seconds test time. No failure diagnosis was needed. Parent coordination subsequently reported independent successful execution of the six exact outbox selectors; that is coordination evidence, not an additional command executed by this reviewer.

The task records passing Rust/Python/TypeScript pressure stories, MCP pressure, runtime/route integration, formatting, lints, dependency boundaries, codegen and declaration checks. No raw logs accompany those claims. I inspected their assertions and source but did not independently rerun Postgres, Python, TypeScript, MCP, codegen, formatting or broad gates. The benchmark and grouped seam are deliberately outside standalone completion; no throughput/RSS qualification is claimed here.

HEAD was rechecked as candidate `9ffdf753ab11ce9b638c906388d3690a6aede249`. No production, test, config or HEAD changes were made by this review. Only this report was written.
