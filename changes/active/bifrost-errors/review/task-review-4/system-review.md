# System resilience review — round 4

Result: **PASS**. No material system-resilience finding proposed.

## Immutable subject and authority

Reviewed Wyrd `526355fb8c58dae6d67d516f98dddcd5c02b0e79` through
`7fe69545d0d8a2ab2a1e3033f52d5091e80d5b0e`; DataFusion
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b` through
`b2d37bc182b9d3e576650a414b364f3d9b5aefe3`; distributed fork
`8fac2a978bdcf8e06457238ebfe9c7f35023c123` unchanged. The approved spec,
R3 remediation and committed evidence govern scope. Repository agent rules,
spec-driven-development and maintainer references were consulted, together
with Bifrost's Oracle/distributed/lifetime authority and the complete DataFusion,
OLAP-serving and analytical-operations-reliability reference slices.

Navigation used CodeGraph first, then the full cumulative changed-file map,
cumulative runtime diffs and owning native source. Review writes only this
report; no candidate edits, benchmarks, host load or heavy verification lanes.

## Deployed paths and producer evidence

| Changed boundary | Producer-to-consumer evidence | System assessment |
|---|---|---|
| Shared logical upper bound | Fork `expr/src/logical_plan/plan.rs:1440-1540`: FULL joins with two nonempty bounded sides and grouping-set aggregates yield unknown; join products and union sums use checked arithmetic. Projection/alias preserve unknown. Scalar rewrite `optimizer/src/scalar_subquery_to_join.rs:379-405` uses unknown to add the existing global aggregate. | Corrects omission of validation before physical execution. Does not add query retries, a worker protocol, durable state or a second bound owner. |
| Native global scalar cardinality | `functions-aggregate/src/single_value.rs:139-192`: state is value plus row count, update and final merge both call the same counting operation; duplicate or NULL rows remain rows. `physical-plan/src/aggregates/mod.rs:2070-2077` requires one input partition for global Final/Single. `aggregates/aggregate_stream.rs:374-438` consumes all input and only finalizes at EOF; update/merge failure produces an error instead of its scalar output. | Validation sees the combined input before dependent scalar output, with one scalar retained per accumulator. Uses existing aggregate codec and partial/final/coalesce boundaries. No user-sized collection introduced. |
| Scalar consumer readiness and remote live IO | Hash join `joins/hash_join/stream.rs:670-704` waits for build-side collection before fetching probe batches. Wyrd `oracle/live.rs:298-317, 533-557, 668-747` resolves the predicate on first poll, partitions the planned route list first, then prunes that share. Each route's fragment uses that static snapshot. | A failed scalar build cannot publish a successful value to the probe. Retaining the original runtime expression preserves re-execution. Partition pruning cannot reshuffle route ownership or duplicate a retained route. |
| Runtime public refusal | Wyrd `oracle/mod.rs:3639-3732` maps first-batch errors, cancels the attempt and request, drops batches and releases admission before returning the error; `3934-3942` preserves the original typed public error. Runtime execution errors remain opaque, unlike caller SQL planning messages. | Failure is scoped to the request/graph, not a shared-server crash or readiness shutdown. Sibling capabilities remain admitted through their existing owners. |
| Test observability | Test-only Scribe plan/footer-row fields in `wyrd-server/src/state.rs`; existing dispatcher/peer service record after authenticated resolution. | No production plan retention, extra dependency, resource owner or durable telemetry path. |

The latest correction changes proof construction, dependency revision fences and
existing negative journeys, rather than execution lifecycle. Earlier cumulative
changes use DataFusion snapshot/aggregate owners and Wyrd's existing fragment
stream, not a replacement scheduler, transport or scalar store.

## Failure and recovery paths

| Credible event | Reachable source behavior | Recovery/availability consequence |
|---|---|---|
| Second scalar row arrives in one batch or a later partial state | `SingleValueAccumulator::add` refuses before final aggregate value is emitted; global aggregate stream returns that failure. | The request fails; it cannot succeed with join/set semantics. The next query uses a fresh execution owner. |
| Scalar source or worker fails while validation consumes input | Native aggregate forwards an input error; Oracle's first-batch settlement cancels and drops the complete graph, then releases admission. | No retry on a different cut and no Interactive fallback; other requests retain their own grants. |
| Cancellation/deadline while waiting for scalar build | Existing `settle_attempt_output` selects cancellation and applies the absolute first-batch deadline; live fragment read selects cancellation/deadline while awaiting frames. | Drop releases nested streams and their reservations through existing graph owners. No new timeout or background task is introduced. |
| Live predicate cannot snapshot yet | `resolved` propagates its snapshot error before constructing/opening a fragment. | Fails this request instead of shipping unresolved process-local state. |
| Live Scribe unavailable | Existing live stream omits only eligible availability loss before rows; loss after rows, security, deadline and protocol failures fail. | Existing best-effort live visibility/terminal policy is preserved; no new success-on-failure or retry path. |
| Process restart/rolling replacement | Candidate adds no persisted query state or recovery protocol. Revision fencing advances with fork pin and lock. | An interrupted query remains a failure; caller may submit a new query against a fresh cut. Storage authority/replay are unaffected. |

## Proof assessment and limits

Committed R3 evidence identifies the exact bound regression test, four native
flag-off Filter/Projection SQL refusals, native aggregate and proto suites,
and existing remote refusal/literal/join journeys. Actual remote refusal source
at `peer_network/live_subquery.rs:239-286` requires the execution-failure code
for duplicate published/live scalar input, FULL JOIN input and GROUPING SETS
input, reusing one client across consecutive failures. This provides useful
request recovery evidence in addition to refusal. Its source does not assert
an immediate per-pod zero-memory measurement; no such stronger proof is claimed.

Source inspection establishes that global aggregation emits no scalar before
input completion and that merged row state reuses the counting owner. Existing
native codec proof and mandatory single-partition Final distribution cover the
seam without a new protocol. The literal/footer journey and direct reset/route
test remain the bounded-work proof; there is no wall-clock performance claim.

No tests were executed by this reviewer. Recorded green commands are evidence,
not independent reruns. The older untraced 604-byte residual observation has no
established cause and is not treated as a candidate leak finding. A broader
integrated capability gate remains change-review work. Conservative unknown
FULL/grouping bounds may add scalar validation work; that bounded correctness
cost is explicitly authorized and does not replace proven singleton MAX/LIMIT
optimizations.

## Findings

Validated within this review lens: the prior producer-side scalar bypass is
closed for the confirmed expanding shapes, with unchanged native execution and
query-scoped error cleanup. No material system-resilience finding; **PASS**.
