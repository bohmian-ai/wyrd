# TASK-006 invariant review

Result: **PASS** within the assigned invariant scope. No proposed findings.

## Subject and scope

- Repository: `/Users/stevenforrester/Documents/GitHub/schema-change-task-6`.
- Base: `5fe833547c66916c8c4868afc8e07d985dcba7d7`.
- Candidate: `c95359fd608fa65d4f0422820751490f61c99202`.
- First review; reviewed the full `git diff 5fe833547...c95359fd6` for the assigned paths and their producers, consumers, and unchanged shared owners. No prior evidence carried forward.
- Scope: attempt-local trace sharing, durable accepted state and authority, retry budget and deadlines, and lease-fenced final publication. This is a specialist result, not the complete task verdict.
- HEAD remained the candidate. The only untracked work was the review directory. No production source was edited.

Read the original task (including its verification claims and diagnoses), approved revision 16 spec requirements REQ-014/015/040/043/045/046 and INV-001–008, AC-014/017/019/020, and the whole-trace decision in `supplements/table-storage-and-correlation.md:798–894`. Governing authority includes the supplied root AGENTS, SQL AGENTS, spec-driven workflow, maintainer guide, Verifier architecture (`architecture/wyrd-design.md:1434–1474`), query-service authority (`architecture/bifrost-design.md:485–518`), and accepted-authority security contract (`architecture/wyrd-security-posture.md:125–158`). No other reviewer report was read.

## Producer-to-consumer traces

### One trace read per attempt

`VerificationEngine::run` (`engine.rs:349`) constructs an attempt clone with a fresh `TraceReads`. Every step/requirement future constructed by `step` and `requirement` clones that attempt, preserving its Arc-backed map. `TraceReads` keys by `TraceId`, not slot or requirement identity; two slots naming one trace therefore converge on one cell. `read` (`engine.rs:744`) releases its synchronous map lock before awaiting the cell. The OnceCell initializes the complete `Result<Arc<Vec<SpanRecord>>, TraceUnavailable>` once, including failures. No check mutates the returned vector. Sequential stages retain the same attempt clone and map until `run` returns, so sharing does not end at a stage barrier.

Different trace IDs have separate cells. A subsequent or concurrent `run` call resets its own map even if the outer engine is reused, so later execution/retry reads do not reuse stale spans or errors. A fetch cancellation does not introduce a usable second collection into a surviving attempt: the existing stage walk aborts its tasks when an engine error/timeout ends the run (`walk.rs:108–159`). There is no background reader, durable cache, or extra authority owner.

`CallerTraces::fetch` remains the hosted source and goes through `ScheduledQueryCaller::authenticated` with `statement_caller`. The latter changes only request ID, retaining principal, tenant, delegation chain and executor. The statement reads exact trace ID, explicitly projects `SPAN_COLUMNS`, orders by start time then span ID, and probes 10,001 rows. Row decoding and >10,000 refusal remain before the collection reaches the engine. Empty rows/missing table produce `NotYetLanded`; denial, decode error and overflow produce `Failed`; query timeout produces `TimedOut`. InMemoryTraceSource is the other production implementation: it supplies its preexisting Arc or `NotFound`, which does not gain availability-retry classification. Test CountingTraces is the remaining implementation inspected.

### Same accepted execution through retries

The accepted execution row remains the producer of claim identity, authority and input. `CLAIM_SQL` (`verifier_executions.rs:370–426`) changes lease/status/attempt coordination fields and returns existing accepted values. `ClaimedRow::into_claimed` (`:3008–3080`) deserializes the stored authority and one closed frozen input shape. `VerifierRunner::dispatch` (`runner.rs:414–496`) restores `Caller::accepted(&run.authority, system)`, passes submitted/grouped input from the claim, preserves owner/subject/run/environment attribution, and enables trace deferral only while `run.attempt < run.max_attempts`. `HostedBlocks::engine` remains the shared direct/queue composition owner; the flag merely configures the existing engine. `Caller::accepted` copies the stored principal/delegation chain; SYSTEM is executor rather than an added grant. No token exchange, grant refresh, account lookup fallback, or new root inference enters this path.

`trace` converts only `NotYetLanded` into `EngineError::EvidenceUnavailable` on such a deferring engine. `engine_failure` maps that through the existing retry outcome. `execute` returns `Transition::Retry` with no StagedResult; the reserved outbox slot drops with the attempt. `settle` uses the existing tenant transaction and `VerifierExecutionQueue::retry`. Its row lock/budget check and lease token prevent a stale attempt from scheduling another retry; the SQL update preserves accepted fields and derives due time from `statement_timestamp()` plus policy duration. Waiting is represented by the row, not a sleeping worker.

`RetryPolicy::default` remains three total attempts, 30/60 seconds. The runner creates a new deadline from configured `execution_timeout` for each attempt (`runner.rs:378–391`), so age/backoff is not subtracted from an accepted lifetime. Generic EvidenceUnavailable/interruption exhaustion still reaches the existing errored transition. On the last trace attempt, only trace deferral is disabled: missing trace becomes an explicit inconclusive requirement and normal `decide` aggregates all final-attempt requirements.

### Final result and siblings

Only `EngineOutcome::Completed` builds a result and stages it through the established completion owner. Retry transitions create no result. The final trace attempt uses the same `result`, `settle`, `Completions` and SQL completion operations as every other verification. `COMPLETE_SQL` compares execution ID and lease token and permits running completion or identical replay; `complete_many` is the existing sibling path. `completions::stage` stages only `Settlement::Applied`; stale completions publish nothing. None of this protocol was replaced or broadened by the candidate.

Direct verification constructs the same HostedBlocks engine with `defer_unlanded_traces: false` (`components/verification/service.rs`), retaining one bounded execution and requirement-level inconclusive. Scheduled/grouped/submitted dispatches retain their existing input and attribution arms. Existing grouped/drift machinery is not used as a second trace loader. Completed executions are outside the queue's pending/retrying/running claim predicates. New spans therefore cannot reschedule or mutate an old result; a new verification makes a new read.

## Acceptance matrix

| Requirement / invariant | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| REQ-045 / AC-019: concurrent and sequential checks, including different slots, share one trace read per attempt | `engine.rs:349–360, 451–506, 703–769`; shared attempt clone and TraceId-keyed OnceCell | U1 `trace_checks_share_one_attempt_read` runs three checks over two slots for the same trace with fetch-yield overlap, distinct selectors, plus another trace | PASS |
| Sharing includes source failure; retries/new runs read afresh; no cross-execution cache | OnceCell contains Result, fresh map at every run; InMemoryTraceSource remains separate source owner | U1 reruns the same engine twice and checks counts for successful and absent trace; precise failed-read and sequential-stage sharing additionally source-proven (not separately exercised by U1) | PASS |
| REQ-043/045, INV-001/002/005/007/008: ordinary accepted tenant/table read, full exact-ID spans, explicit projection/order/overflow | `CallerTraces::{statement,fetch}`, `HostedBlocks::engine`, `statement_caller`; no authority filtering replaced by Card/workload predicates | J1 covers cross-workload/untagged/old/future/other-tenant/other-trace, denial, 10,000/10,001; I3 covers denial/overflow classifications | PASS |
| REQ-014/015/040/046, INV-003/004, AC-014/020: accepted identity, authority, inputs/idempotency remain unchanged across retry/restart | Existing claim SQL and ClaimedRow conversion; `dispatch` restores accepted Caller and frozen input | I1 real engine: absent table then empty table, runtime restart, final arriving evidence; compares stored accepted snapshot including authority, inputs, principals, key/hash, exact identities and max_attempts | PASS |
| REQ-046: three attempts, 30/60-second DB due times; waiting releases worker; no total-lifetime expiry | `RetryPolicy`, `retry`, RETRY_SQL, runner deadline construction, existing claim-loop ownership | I1/I2 inspect attempts and stored due-time deltas; I1 restarts runner; SQL arithmetic regression is recorded by task | PASS |
| AC-020: publish no intermediate result; last absence completes normal per-requirement decision | `trace` strict variant/flag match; completed-only result construction; existing decide, completion fence and staging | I1 checks no writes before completion and one final result; I2 trace-only inconclusive / all mixed failure / pass_rate pass with one verdict row per result | PASS |
| REQ-045 / AC-019: nonempty trace judged immediately; completed result immutable after late spans | Successful trace bypasses deferral; completed rows unclaimable; fresh map for a new run | J2 starts with nonempty failed trace on attempt 1, adds span, new direct verification passes while stored old result remains failed | PASS |
| REQ-046 / AC-020: failed judgment, invalid/auth/oversized evidence and timeout retain classification | `trace` retries only NotYetLanded; `engine_failure` keeps timeout terminal; direct flag false | I3 direct inconclusive/no enqueue, queue negative cases settle once; existing cancellation/deadline and generic-exhaustion tests remain in owner module; defaults asserted | PASS |
| INV-006 and task non-goals: reuse durable server owners without new queue/cache/completion protocol | Full reviewed production diff: engine attempt map plus hosted mode flag; existing reader, SQL queue and completion owners retained | Static diff and owner/caller tracing | PASS |

## Independent reuse comparison

- Existing `InMemoryTraceSource` storage cannot provide attempt consistency for server queries; it owns independently inserted local traces and has no accepted-caller query authority. The new attempt map belongs on VerificationEngine, where concurrent and sequential checks already share their lifecycle. It extends the owner, does not replace TraceSource or duplicate the query service.
- Existing `walk` already owns stage scheduling/cancellation; the candidate reuses it. No new orchestration loop is added for sharing or retry.
- Existing `EngineError::EvidenceUnavailable`, runner transition mapping, SQL RetryPolicy/claim/retry, and completion fence cover all durable availability behavior. The candidate extends only the trace-to-classifier seam and final-attempt policy, rather than constructing a second queue, timer, submission or publisher.
- `HostedBlocks` remains the only hosted engine constructor for both direct and queued callers. Removing acceptance-anchor arguments from CallerTraces eliminates the obsolete narrowing rather than retaining parallel reader modes.

## Findings and verification limits

Proposed finding ledger: **empty**. No source-proven material violation was established in this scope.

No Cargo or server tests were run by this specialist, as instructed. The orchestrator’s independent `verification.md` records exact U1 1/1, the engine module 24/24, exact deferral test 1/1, and server build passing. Its shared-fixture consumer run records all trace consumers passing, with two code-check failures awaiting a serial rerun; those failures are not resolved or waived by this scoped report. Other task-reported green runs were treated as available verification claims, not independently reproduced execution. U1 does not separately repeat an erroring trace through several checks or place repeated trace checks in different stages; the actual map/OnceCell lifetime proves those paths, and the missing extra test variants do not establish a behavioral defect. This report does not decide the separate repository/TDD evidence obligations or other reviewers' assigned surfaces.
