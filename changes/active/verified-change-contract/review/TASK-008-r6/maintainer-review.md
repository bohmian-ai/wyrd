# Maintainer Review

## Subject and scope

- Immutable remediation range:
  `1d05642bf2c4d824de1aec27286048ec79b37e25..f8d6945041467d52020024d311ce8831a45ff4a1`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Remediation task:
  `changes/active/verified-change-contract/review/TASK-008-r5/TASK-008-CLOSEOUT-R4-audit-handoff-and-owner-shape.md`
- Closure scope: `FIND-TASK-008-CLOSEOUT-16`,
  `FIND-TASK-008-CLOSEOUT-17`, and regressions introduced by this range.
  Earlier accepted code was not reopened. The deferred full default
  `bench:capacity` run (`FIND-TASK-008-CLOSEOUT-13`) was not treated as a gap.

The candidate remained at `f8d6945041467d52020024d311ce8831a45ff4a1`
through this review.

## Changed-surface coverage

| Changed surface | Owner and callers traced | Relevant proof inspected | Maintainer assessment |
|---|---|---|---|
| `OracleQueryAudit::stage`, `OracleAuditWriter::run`, and `PENDING_GAUGE` in `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:53-178` | `OracleQueryAudit` remains the concrete process owner. Construction in server boot and `AppState` shares one owner with Oracle, tenant-tripwire, and direct-verification consumers. The writer remains the only receiver and decrements ownership only after commit or counted loss. | The real held-chain-head test reads the same process recorder; the supporting arithmetic test parses the same metric family used by replica scrapes. | PASS. The metric is maintained beside the existing atomic owner, follows the same enqueue/commit lifecycle, and adds no second state owner, queue, or public configuration. Names and rustdoc state the pre-commit ownership invariant and update ordering. |
| `Backlog::with_replicas` and `Queue::backlog` in `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:176-301` | `Backlog` owns the combined drain decision value; `Queue` owns durable reads. The only production composition is `Deployment::drain`, which scrapes replicas before reading durable state. | `pending_decisions_add_to_staged_audit_rows`; `the_audit_backlog_holds_from_a_pending_decision_until_its_publication`; existing Scribe backlog and drain-edge tests. | PASS. The method name, types, and documentation make the replica-local plus durable composition discoverable. Removing the audit timestamp cut is explained at the durable owner, while the still-relevant `stopped` argument remains visibly owned by Forge filtering. |
| `Deployment::drain` in `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:395-424` | Read from `Deployment::run`; it owns replicas and the durable queue. | Held-commit handoff proof and exact drain-edge proof. | PASS. The scrape-before-query ordering is local to the lifecycle owner and its short comment explains why temporary double-counting is deliberate and a miss is prohibited. |
| `Deployment::stop_replicas` and `Benchmark::clean_up` in `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:427-454` and `capacity/main.rs:581-604` | `Benchmark` owns client-before-replica cleanup; `Deployment` owns the replica collection; `LocalServer::stop` owns one process. The sole production caller follows that ownership chain. | `a_slow_replica_stop_leaves_the_runtime_free` now exercises `Benchmark::clean_up` through the inherent deployment method and asserts progress, reap, log retention, result ordering, and client-cleanup outcome. | PASS. The former ownerless workflow is deleted. The inherent method consumes the collection with `mem::take`, documents the empty post-state and cancellation behavior, and preserves the blocking boundary without adding a manager, trait, or helper layer. |
| `Queue::unconnected` and the expanded cleanup proof in `capacity/evidence.rs:229-240` and `capacity/main.rs:907-993` | Test-only constructor supplies the otherwise unused queue field needed to assemble the real `Deployment`; the proof calls the production cleanup owner. | Slow-stop proof source and its stand-in process lifecycle. | PASS. The helper is `cfg(test)`, narrow, fallible, documented, and avoids introducing a mock queue abstraction merely to construct the existing owner. The longer scenario earns its setup because it proves the caller-visible lifecycle rather than a detached helper. |
| Held-commit integration proof in `capacity/evidence.rs:354-588` | Helpers remain beside their only caller and name the three observable phases: blocked writer, publication, and drain read. It uses the public client, real server state, Postgres chain-head fence, and real publisher. | Pending-only, pending-to-staged, late-created staged row, and post-publication assertions. | PASS. Although necessarily multi-stage, the test exposes the scenario in execution order and asserts outcomes at each ownership handoff; it does not add a reusable harness for one case. |
| `architecture/bifrost-design.md:587-598` | Read-audit authority beside the existing non-blocking audit and publication contract. | Compared with `OracleQueryAudit` gauge lifecycle and capacity consumer. | PASS. The documentation names the metric and defines exactly when it is nonzero without changing the audit contract or narrating task history. |

Generated declarations are not applicable: the range changes no public wire,
Python, TypeScript, schema, or generated surface.

## Finding closure

### `FIND-TASK-008-CLOSEOUT-16`

Closed. Replica-set shutdown is now an inherent operation on `Deployment`,
the concrete owner of the live replica collection. `Benchmark::clean_up`
retains the higher-level client-before-replica ordering, and
`LocalServer::stop` retains single-process termination. The source and focused
proof keep newest-first stopping, ordinal result alignment, per-ordinal logs,
join/process error conversion, Tokio `spawn_blocking`, and documented
cancellation partial progress.

### `FIND-TASK-008-CLOSEOUT-17`

Closed. `OracleQueryAudit` exports the already-owned pending count at the
producer, `Deployment::drain` combines each replica's value only at the
existing backlog decision boundary, and `Queue::backlog` counts every staged
row above its publication watermark. The scrape-before-durable-read order is
explicit, so a commit racing across the handoff may be counted twice for one
poll but cannot disappear. The held-chain-head proof exercises the pending,
late staged, and published states through the actual owners.

## Material findings

None.

## Calibration notes

- `Deployment::stop_replicas` is visible outside `step.rs` only because its
  owner is consumed from the sibling `main.rs`; this does not create an
  external library contract or a second implementation.
- `Queue::unconnected` constructs deliberately unusable IO state, but only
  under `cfg(test)` and only so the cleanup proof can assemble the real
  deployment without inventing a queue interface. Its constrained use is
  cheaper to maintain than a mock abstraction.

## Verification limits

- Static review covered every materially changed symbol, its owning module,
  production callers, and relevant tests.
- Before the orchestrator requested discovery reviewers perform no further
  verification commands, one already-started focused command completed:
  `evidence::tests::pending_decisions_add_to_staged_audit_rows` and
  `step::tests::a_backlog_drains_only_within_the_limit` both passed. No other
  Cargo, mise, or codegen command was run by this reviewer.
- The default `bench:capacity` execution remains deferred to integration and
  no AC-040/AC-041 empirical qualification is inferred here.
- The known intermittent
  `verification_runtime::two_bindings_share_one_client_observation` failure is
  on an untouched path and is tracked separately for the integrated branch;
  it does not count against this remediation range.

## Overall result

**PASS**

The range closes both assigned findings with repository-native owners,
maintainable naming and documentation, direct lifecycle tests, and no material
maintainer regression introduced by the remediation.
