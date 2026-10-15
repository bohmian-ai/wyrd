# Round 6 — independent system-resilience review

Result: **PASS**. No material system-resilience finding proposed.

## Subject and scope

Reviewed Wyrd `945a1548c47897dc94583169a6ba04163a73a749` →
`95b350d078226e811ac298230c233bf935975d42`, including the cumulative
runtime changes, rather than only the last remediation. Native DataFusion
scope is `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b` →
`b8ec4b1d291cd8e371bb88fc76959d91e8503a1d`. Both current HEADs were
confirmed unchanged at inspection. No original task existed; approved
`changes/active/bifrost-errors/spec.md`, owner directions and the remediation
packet provide acceptance authority. Latest R5 evidence was inspected alongside
the actual source and cumulative diff. No other round-6 discovery report was
read.

Applicable authority: AGENTS.md, architecture/agent-rules.md, Wyrd design and
doctrine, Bifrost design (live snapshot/source loss, distributed ownership,
failure-terminal and admission contracts), reference router, spec-driven
development, maintainer style and TESTING.md. CodeGraph was used before Wyrd
symbol navigation and expanded to the relevant lifecycle consumers.

## Deployed paths and ownership

| Path | Source evidence | System effect and owner |
|---|---|---|
| Caller SQL → logical planning → physical planning | `oracle/mod.rs:3018–3036,3735–3801` | Typed logical caller faults carry actionable messages; physical faults remain redacted. Resource exhaustion keeps precedence. This changes the request response, not process health or service ownership. |
| Analytical-configured planning | `oracle/analytical.rs:6645–6679`; `resources.rs:4090–4123` | Existing distributed planner and LiveUnionBoundary remain installed. Two optimizer passes permit rewritten equality to become a hash key. No successor attempt, topology override or local fallback is introduced. |
| Planned live source → first poll → Scribe fragment | `oracle/live.rs:298–317,331–384,441–477,533–557,668–760` | Planned routes are partitioned first, then narrowed by a per-execution static snapshot. No IO opens before polling. The existing bound execution lock supplies grant, cancellation, deadline and dispatcher; snapshot introduces no background task, cache, retained rows or new lease. |
| Scribe receipt → authenticated scan → batches/footer | `wyrd-server/src/oracle/peer_service.rs:286–391` | Tenant, fences, binding and digest validation precede follower creation/IO. Query-owned follower execution and the existing lazy stream remain the owners. New plan/footer inspection state is test-support-only; production gains no plan retention or new lock. |
| Scalar bound → rewrite → aggregate execution | native `expr/src/logical_plan/plan.rs:1449–1520,2909–3011`; `optimizer/src/scalar_subquery_to_join.rs:363–405`; `functions-aggregate/src/single_value.rs` | A sound singleton proof bypasses extra work; an unknown uncorrelated bound uses the existing global aggregate mechanism. Partial state carries value and row count, so rows on separate partitions are counted together. A second row becomes a query execution error, not a panic or service shutdown. |
| Latest fixed-expression repair | native `plan.rs:is_fixed_value`; `expr/src/tree_node.rs:45–105` | Eligibility remains positive; volatility is refused before recursive child inspection. Native child traversal includes CASE conditions/results/base/ELSE and the sibling pure expression operands. No expressions are evaluated during the proof. This is planning CPU with no resource or protocol lifecycle. |

## Failure and recovery assessment

| Credible failure/interruption | Producer → consumer evidence | Boundary, surviving state and recovery |
|---|---|---|
| Scalar value is not ready when the live stream is polled | Native `physical-expr/src/scalar_subquery.rs:snapshot` returns an error; `live.rs:677` propagates it before opening routes. | The query fails. It cannot ship an unresolved process-local scalar container to another pod. No fragment was opened by this partition; ordinary attempt cleanup releases query ownership. The reusable plan retains the runtime expression and snapshots anew on re-execution, rather than retaining an old literal. |
| Scalar input has more than one row, including rows split across partitions | `SingleValueAccumulator::update_batch`, `merge_batch`, `add`; shared scalar `build_join`. | Native execution error reaches the same first-batch/stream failure handling as other operator errors. Equal values and NULL values still count as rows. No persistent data changes, retry loop, child-process crash or new failure handler is added. |
| Cancel/deadline while waiting for scalar/build or first output | `oracle/mod.rs:3639–3732` races first batch with cancellation/deadline, cancels both attempt/request, drops batches and calls `release_error`; `admission.rs:1509–1520` releases root/resources. | One request/attempt terminates. Dropping its batch stream tears down its dependent work. Admission release precedes the refusal; no task-level retry amplifies it. Unrelated queries keep their own cancellation/admission owners. |
| Live source unavailable before rows or lost after rows | `LiveFragmentRead::into_stream`, `live.rs:685–753`, retaining existing decoder/availability classification. | Availability loss before rows records known live degradation and proceeds to the next route. Loss after rows, integrity/schema/tenant/resource failure, missing footer or cancellation fails the query. Snapshot/narrowing does not broaden which failures may degrade. |
| Client drops an unpolled or partially consumed live query | Lazy `execute`/`into_stream`; `query_stream.rs:400–425,528–559`; `LeaderStreamOwners::drop`. | An unpolled source opened no fragment. Dropping a polled source drops its active transport and scan stream; leader ownership revokes Analytical work before dropping the profile. Existing guards retain the cleanup duty even if asynchronous settlement is interrupted. No detached task was added. |
| Operator error after some output, including remote aggregate error | `query_stream.rs:645–722`; `settle_and_finish_stream` and `settle_failed_cancellation`. | Failed terminal cancels request and stream, settles the Analytical owner and releases admission. Cleanup that is not confirmed cannot produce a success terminal. Earlier frames are not a complete usable result under the existing protocol. The mapper remains opaque for runtime failures. |
| Pod dies or is replaced while selected work is running | Bound endpoint/worker fence in `live.rs:358–383`; existing authenticated follower validation; existing terminal transport/error handling. | No new durable state must recover. A selected Analytical transport failure remains terminal, without topology re-selection or fallback. Later caller queries acquire a fresh cut/identity through existing discovery. Scribe durable data and publication owners are untouched. |

The latest fix preserves both consumers of the proof: correlated admission can
accept ordinary fixed outer-value expressions, while uncorrelated scalar
rewriting still validates a row-dependent expression at runtime. Conservative
unknown expression forms do not create a process-wide refusal. This distinction
matters because correlated analysis may reject an unproven shape rather than
install runtime validation; the R5 pure-family source and positive journey
exercise that consumer explicitly.

## Verification and limits

Inspected R5's exact-command evidence and the journey assertions, including the
new grouped correlated CASE lookup with an unmatched row, six scalar-refusal
queries through a real public client, literal/fragment/footer evidence and
the compact query corpus. The refusal journey requires the stable execution
error code for each query, then uses normal cluster shutdown. Its shutdown
helper releases armed pauses, cancels/joins active statements and calls the
real cluster shutdown (`peer_cluster.rs:345–366`). The latest client reuse
change fixes duplicated setup identity; it does not alter retries, assertions,
timeouts or acceptance of an execution failure.

Recorded green evidence includes the three remote journeys, corpus, focused
snapshot/mapper and codec selectors, native proof/rewrite/aggregate/proto
checks and formatting/lints/boundary checks. This review did not execute them
again. Their recorded outcomes are supporting evidence, not a substitute for
the ownership/source analysis above. The focused happy/negative journeys do
not independently prove every crash/cancellation path; those paths retain the
existing lifecycle mechanism, whose source was inspected. No latest change
modifies recovery ordering or requires a new recovery protocol.

No benchmark, host-load experiment, build, broad lane or fault injection was
run. Performance is assessed structurally: static pruning remains available,
routes are dispatched at most once per partition assignment, resolved values
travel as native literals, and no extra per-row Wyrd machinery was introduced.
The additional optimizer pass is bounded planning work; no latency measurement
or universal performance guarantee is asserted.

## Material findings

None. The cumulative change remains within the existing request/attempt,
admission, stream, authenticated fragment and query-memory owners; the latest
native proof correction does not introduce a new runtime failure domain.
