# System resilience review

Result: **PASS** for the changed deployed and failure paths. No material system-resilience finding proposed. This does not assert complete SQL semantic acceptance, which the independent task and DataFusion reviews own.

## Immutable subject and scope

- Base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Candidate: `7d6a1ff95b3dc09315e3af236ab9906b305e11d0`.
- Approved intent: `changes/active/bifrost-errors/spec.md`, original owner directions, and the R1 remediation at `review/task-review-1/bifrost-errors-R1-materialize-live-subquery-predicates.md`.
- Complete cumulative diff reviewed; prior candidate used only to locate remediation. Source remained unchanged. HEAD was the candidate at inspection; only the new review directory was untracked.

Authority: AGENTS.md, architecture/agent-rules.md, spec-driven-development and maintainer-style references; Wyrd design Bifrost boundary and doctrine service/runtime boundary; Bifrost design immutable planning, distributed execution, and live-source failure contracts; references/domain/{datafusion,olap-serving,analytical-operations-reliability}.md. CodeGraph was used before code navigation.

## Deployed-path evidence

| Path / owner | Changed behavior and inspected evidence | System consequence |
|---|---|---|
| Public SDK/HTTP/MCP → Oracle SQL planning | `oracle/mod.rs:3025-3031,3735-3831` splits SQL diagnostic mapping from physical-plan mapping, preserving resource failures | A request receives a repairable refusal; dependency/internal failures remain request failures. No component restart, new queue, or admission bypass is introduced. |
| Oracle planning configuration → leader, admitted execution, follower and stages | `resources.rs:4108-4124` is the existing shared `OracleSessionShape::apply` owner; callers and retained-shape tests in `oracle/{mod,exec,follower,analytical}.rs` use its configuration | Second logical pass allows rewrite-produced equality to become an existing hash-join key. No second physical planner, fallback attempt, new runtime, or durable state is introduced. This affects planning cost across callers, but no throughput claim was inferred from the comment. |
| Scalar/build producer → live leaf | Pinned DataFusion `physical-expr/src/scalar_subquery.rs:170-187`; `physical-plan/src/joins/hash_join/{exec,stream}.rs` build/filter publication and wait states | Scalar snapshot returns a literal or fallible pending-state error. Hash-join probe stream is constructed early, but the stream state machine waits for build and partition-bounds delivery before polling the probe. Deferring live snapshot until first poll therefore uses established producer ordering. |
| Oracle live leaf → local/remote Scribe | `oracle/live.rs:298-317,441-479,533-557,668-752` retains Pre static and Post dynamic filters; snapshots and route-prunes lazily | No remote fragment opens at `execute`. Each partition owns a fixed planning-time route share before runtime narrowing; a snapshot cannot transfer a route between output partitions. Each route fragment carries one immutable predicate snapshot. The original dynamic expression stays on the plan. |
| Scribe fragment execution → bounded batches and completion → Oracle | `wyrd-server/src/oracle/peer_service.rs:340-389`; `oracle/dispatcher.rs:1934-1983`; live footer decoder | Existing authenticated follower execution, query pool, cancellation, deadline and footer protocol remain owners. New plan/row observations are test-support-only; production does not retain a second encoded plan or global runtime predicate. |
| Dependency rollout / peer codec | Cargo pins and `oracle/codec.rs:32-59` revision-bound fingerprint | New revision stays in the single native dependency universe and changes the fragment fingerprint. Mixed old/new peer incompatibility remains a structured rejection rather than decoding an unqualified fragment. No compatibility promise or migration state was added. |

## Failure and recovery paths

| Reachable interruption | Evidence | Boundary, release and recovery assessment |
|---|---|---|
| Drop before first live poll / build failure before probe | Live `execute` only constructs the adapter; first-poll body checks bindings and `grant.ensure_live` before `resolved` or dispatch | No Scribe attempt, snapshot or follower lease has opened. Existing query/plan drop releases producer resources. Snapshot failure is a request/partition error, not permission to crash a shared server. |
| Pending scalar snapshot | `resolved` propagates `snapshot_physical_expr` error before fragment construction; fork snapshot has no panic | No partial remote work starts. The original expression remains available for normal reset/new execution; the typed dependency failure reaches ordinary query termination. |
| Cancellation / deadline while opening peer stream | Dispatcher open is `select!`-bounded by cancellation and original remaining deadline; live caller rechecks `grant.ensure_live` after refusal | Cancellation/deadline cannot be mistaken for optional live-source loss. No retry with new authority or extended deadline is introduced. |
| Cancellation / deadline after open | Live read selects cancellation and deadline while awaiting every frame; drop owns the returned fragment stream | The query fails and drops the open stream, retaining the existing Scribe snapshot/lease release path. New snapshot variables are query-local and have no detached task or persistence. |
| Scribe unavailable before rows | Existing live `is_availability_loss` classification and footer decoder retained | The route is omitted with recorded degradation only for eligible availability loss before rows. Filtering at Scribe can legitimately emit zero rows; successful completion still requires footer validation. |
| Scribe lost after rows / malformed frame / missing footer | Existing `decoder.rows()`, frame ordering and completion checks retained | Request terminates failed; it cannot return successful partial data. No successor query attempt or alternate source fallback was added. |
| Graceful Scribe shutdown / process restart | Peer service shutdown token and existing follower lifecycle; route assignments still carry node identity, writer epoch and deadline | The open stream ends, or new epoch/cut admission refuses stale authority through the existing path. Nothing in this change modifies WAL, publication, persistence, restart identity or source authority. |
| Re-execution / recursive reset | Live plan preserves unsnapshotted predicate; scalar test clears results and resolves new H2/H3 values; DataFusion hash join `reset_state` clears runtime filter/build accumulator | The leaf does not cache the materialized remote predicate across executions. Broader recursive SQL semantics are not established by the leaf-only test. |

The existing join and leader FilterExec remain authoritative, so a snapshot of a progressively tightening TopK filter can lose pruning opportunity but cannot introduce extra accepted rows. DataFusion controls eligibility for join pushdown, including preserved probe-side semantics; Wyrd reports filters unsupported and does not remove authoritative operators.

## Proof assessment and limits

- The committed remote scalar journey directly checks selected route/fragment count, exact footer output rows, and encoded static/scalar literals (`peer_network/live_subquery.rs:110-154`): this is credible bounded-transfer proof without a benchmark or timing threshold. It does not prove general planning throughput.
- Existing `peer_network/analytical.rs:125-246` tests one-attempt peer loss/cancellation with follower and leader graph release. `:1180-1292` drives client drop, deadline, and loss after delivered rows for remote live streams; `:1449` onward covers graceful shutdown with a paused reader. These exercise unchanged lifecycle owners under the changed session configuration. The evidence records the entire Oracle lane passing 57/57; no fresh heavy rerun was performed here.
- The earlier MCP analytical run retaining 604 bytes has **no established diagnosis**. A later green run is not evidence explaining that failure, nor evidence that a particular edit repaired it. No test assertion, wait, retry or skip was weakened in the cumulative change to conceal it. With unchanged release owners and no source-proven causal regression, this is an explicit verification limit, not a new speculative implementation finding. It must not be described as a diagnosed or repaired race.
- The reported multi-row scalar correctness limitation is not resolved by resilience tests or by retaining the leader join. It is routed to task/DataFusion acceptance review rather than claimed closed here.
- No benchmarks, synthetic load, raised concurrency, implementation changes or broad verification reruns were performed. This review establishes source-level failure ownership and checks committed proof; it does not claim performance measurements, exhaustive crash qualification, or a root cause for the untraced memory observation.

## Findings

Validated discovery proposal set from this scope: empty. Overall system-resilience result: **PASS**, with the verification limits above.
