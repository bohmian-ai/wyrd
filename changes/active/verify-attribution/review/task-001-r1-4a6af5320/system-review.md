# TASK-001 R1 system-resilience review

## Subject and authority

- Original base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`.
- Candidate: `4a6af5320ab8c236fbe640ff0dc5ddf544bdcde2`.
- Candidate tree: `036d647413d0acfd1afdebff2b7057b45ac00609`.
- Approved specification: `changes/active/verify-attribution/spec.md`, revision 3.
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`.
- Prior verdict: `changes/active/verify-attribution/review/task-001-827ec9f15/verdict.md`.
- Remediation task: `changes/active/verify-attribution/review/task-001-827ec9f15/TASK-001-R1-close-trace-continuity-gaps.md`.
- Governing runtime authority reviewed: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/bifrost-design.md`, `architecture/references/domain/telemetry-observations.md`, and `architecture/references/languages/testing-workflows.md`.

The candidate commit and tree matched the review subject when this report was written.

## Deployed paths reviewed

| Path | Source evidence | System assessment |
| --- | --- | --- |
| Request to tenant span copy | `crates/wyrd/wyrd-server/src/state.rs:1573-1657` composes one `scribe_outbox` and one `ServerSpanSink` for each deployed server process; `crates/wyrd/wyrd-server/src/scribe_outbox.rs:654-685` admits a projected tenant copy with `try_acquire_owned` and carries the permit on `ScribeWrite::ServerSpans`. | Request processing never awaits Scribe. At most 2,048 tenant-copy items from that server retain permits; overload drops and counts only the copy. |
| Shared sibling writes | `crates/wyrd/wyrd-server/src/scribe_outbox.rs:96-129` keeps audit, capture, result, and span writes in the existing `ScribeWrite`; only `ServerSpans` owns a permit. `crates/shared/wyrd-runtime/src/outbox.rs:160-205` stages through the non-blocking channel. | Audit, gateway capture, and verifier-result admission are not starved by the span permit pool and retain their existing non-blocking semantics. They can remain queued behind a failed tenant slice, as approved, and resume without request failure. |
| Scribe outage and retry | `crates/wyrd/wyrd-server/src/scribe_outbox.rs:500-588` reads the authoritative live-ready Scribe roster on every attempt and returns retryable `Unavailable` when none exists. `crates/shared/wyrd-runtime/src/outbox.rs:275-443` retains the exact failed tenant slice, applies bounded backoff, and releases items only after success or counted abandonment/loss. | A missing peer does not crash the process or consume request tasks. The same in-memory items and span permits survive retries; newly staged siblings remain accepted. |
| Recovery after a Scribe joins | `ScribePeers::channel` re-reads `ClusterRegistry::snapshot` on every retry rather than freezing the boot-time roster. `crates/wyrd/wyrd-testing/src/bifrost/cluster.rs:251-260` makes the last role-separated node the sole Scribe, `start_spec_delayed_last` leaves it unbooted, and `restart_node` starts that same configured node. | The test represents the production split-role topology: an Oracle-only server initially has a valid peer route but no live Scribe, then discovers the joined Scribe without rebuilding its outbox. |
| Shutdown and cancellation | `crates/wyrd/wyrd-server/src/app/server.rs:850-895` drains the process Scribe outbox before Bifrost role shutdown; `Outbox::shutdown` closes admission, retries to the deadline, cancels the writer, counts remaining loss, and drops retained `OwnedSemaphorePermit`s. | Graceful shutdown drains when possible; deadline expiry or cancellation releases every retained span permit exactly once and reports loss rather than hanging the shared server. Abrupt process death retains the already-approved in-memory-loss boundary. |
| Queue context byte ownership | `crates/shared/wyrd-queue/src/producer.rs:545-564` includes producer-context string bytes in the record reservation; `crates/shared/wyrd-queue/src/queue.rs:559-599` transfers the selected first-32 contexts into the sealed batch's existing budget guard. | Ambiguous transport retries keep both frame and context bytes charged. ACK, terminal refusal, or shutdown drops the owning guards; no new retry or recovery owner was introduced. |
| Linked ingest | Gate still attaches validated producer links before starting `bifrost.gate.write`, while the existing tenant route opens only for a sampled producer parent/link. The remediation and later merges do not change this runtime path. | The remediation does not introduce a second route opener, parent rewrite, blocking handoff, or new failure propagation path. |

## Failure and recovery proof assessment

`pg_verification_runtime::sustained_scribe_outage_caps_tenant_spans` at
`crates/wyrd/wyrd-server/tests/integration/pg_verification_runtime.rs:3539-3660`
uses the existing role-separated `WyrdTestCluster`, not an in-process fake:

- the two Oracle-only servers boot while the sole Scribe node remains an
  unbooted cluster slot;
- repeated authenticated HTTP verifier executions create sampled tenant span
  copies and verifier results until the production Scribe loss counter proves
  the 2,048-copy cap rejected overflow;
- gRPC health and authenticated MCP `tools/list` still respond during the
  outage, while the successful verifier requests themselves prove continued
  HTTP availability;
- the test retains `Arc::clone(&oracle.state().scribe_outbox)` before recovery,
  starts the delayed Scribe, and settles that same handle;
- it then proves exactly 2,048 outage copies and every verifier result were
  published, submits another traced request, and proves a post-recovery copy
  publishes without increasing loss.

That is a credible test of the deployed peer-membership and retry topology.
Using the cluster harness is necessary and simpler than adding a production
fault switch to `WyrdTestServer`; the deviation recorded in the remediation
task does not weaken the required proof.

The test does not separately count recovered audit rows. That is not a material
gap in this remediation: each authenticated execute traverses the ordinary
authorization boundary during the outage, audit uses the same non-permitted
outbox admission as the verifier results, request success proves it remains
non-blocking, and the candidate did not change audit production or projection.

## Merge impact and verification limits

The post-remediation merges do not alter `ScribePeers`, `ScribeSink`,
`ServerSpanSink`, the generic outbox retry owner, the cluster delayed-node
mechanism, or the body of the outage proof. Their only task-adjacent runtime
change in `state.rs` adds unrelated Scribe fragment test inspection and removes
gateway secret-key state. The outage test's metric parser was changed from an
`f64` cast to direct `u64` parsing before the merges; the recorded remediation
evidence states the named test passed after that lint-driven correction.

The focused outage test and SDK journeys were not rerun at candidate
`4a6af5320`; only format, lint, typecheck, codegen, and docs checks are recorded
after the merges. This is a verification limit for the orchestrator to
reconcile, especially for the merged Python and TypeScript contracts, but
source inspection found no system-runtime change that invalidates the earlier
outage proof. Per assignment, this reviewer did not start another Cargo or
`mise` process while parallel review was active.

## Material findings

None.

## Overall result

**PASS**

The remediation closes the prior same-outbox recovery and sibling-availability
finding with the existing production topology and owners. Dependency outage,
retry, recovery, shutdown deadline, and cancellation retain the approved
non-blocking and observable-loss boundaries without new machinery.
