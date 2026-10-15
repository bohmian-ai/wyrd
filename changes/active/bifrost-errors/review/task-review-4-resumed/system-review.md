# System resilience review — resumed round 4

Result: **PASS**. No material system-resilience finding proposed.

## Immutable subject and authority

Reviewed task cumulative Wyrd `945a1548c47897dc94583169a6ba04163a73a749` through
`17697dc8c2740213565e5f913cde1edfb0cb8ce5`, retaining original task history
from `526355fb8c58dae6d67d516f98dddcd5c02b0e79` and pre-merge evidence at
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


## Resumed integration revalidation

The owner authorized this new immutable subject after the upstream merge. The
old report remains untouched. Compared integration baseline → current candidate
and pre-merge candidate → current candidate, not merely the latest fork patch.
The Bifrost task cumulative changed-surface set is retained; no pre-merge →
current delta changes vala-bifrost-redux, its bounds, aggregate, snapshot,
admission or analytical lifecycle. The DataFusion and distributed candidates
are unchanged, so the native producer/runtime evidence above remains current.

| Shared integration seam | Actual source inspection | Result |
|---|---|---|
| AppState/boot gateway composition | state.rs removes only the gateway-specific managed-secret key field/method, keeps Oracle/Scribe and process shutdown/outbox owners. boot/mod.rs::attach_config_fields passes existing auth sealing key to gateway credential resolver. Bifrost task-only Scribe observations retain their cfg boundary. | No new Oracle resource/lifetime owner or scalar state sharing. PASS. |
| Real test-server startup | server.rs:4982-5038 constructs AppState from the same Bifrost runtime and shutdown token, installs auth before gateway credential resolution then constructs gateway engine. Removed mounted gateway test-key directory belonged to gateway keys, not Bifrost source files/runtime. | Real-peer journeys retain production Bifrost runtime/public client owners. PASS. |
| MCP discovery and query dispatch | mcp/mod.rs catalog still begins with Bifrost descriptors; only gateway write discovery is changed. Private/zero-TTL cache metadata preserves caller-dependent discovery. call_tool:243-280 retains shutdown-observed TaskToken, verified caller and same Bifrost list/describe/query dispatch. | Query refusal, terminal and cancellation reach existing Bifrost owner. PASS. |
| MCP test payload integration | Connectivity/discovery reorder gateway writes and add cache/schema assertions; operator-create payload gains request wrapper. Bifrost query tests/execution unchanged. | No reduction of Bifrost query/error proof. PASS. |
| Verification lanes | mise additions concern gateway/Jev and agent-framework MCP lanes; Oracle owning lane remains repository-Postgres wrapped and --test oracle -P journey --run-ignored=all, retaining live-subquery tests. Rust integration lane retains query_coverage/query_bifrost. | Existing environment-owning task proofs remain reachable. PASS. |

No benchmarks, new tests or heavy lanes ran in resumed discovery. Recorded
green results predate main integration and cannot certify post-merge execution;
this pass revalidates source/seams only. Later integration verification must
retain that limit. Native scalar correctness review is independently owned by
other discovery roles; this system PASS does not certify that every possible
singleton-proof defect is absent.

Resumed system-resilience result: **PASS**, no proposed findings.
