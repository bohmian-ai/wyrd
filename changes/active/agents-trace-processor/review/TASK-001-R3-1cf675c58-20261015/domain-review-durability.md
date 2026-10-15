# Durability and persistent identity review

Result: **PASS**. Material proposed finding ledger: **empty**.

## Immutable subject and boundary

Reviewed original base `dac4c340f835676adc29f3dcc01f98e1d3d51465` through candidate
`1cf675c58d01cba0e714102c75cb7d89e7a678b5`, including the authorized imported-main
subject/actor seams. The previous-candidate diff and imported-main contrast were
navigation aids, not replacement review bases. HEAD matched the candidate before
inspection and at the closing guard. Only this report is owned by this reviewer.

Boundary: actual framework span completion → stock exporter batches → bounded
current-call managed flush → native writer drain → authenticated canonical Gate
dispatch → Scribe stamping, WAL and memory/durable replay fences → publication
and public query. Native explicit batch IDs and the process Scribe outbox were
inspected as sibling identity owners. This is a durability-domain verdict, not
the whole-task acceptance verdict.

## Authority and source coverage

| Authority / obligation | Reviewed source and consequence | Result |
|---|---|---|
| Approved spec r2 REQ-011, INV-001: generic telemetry retains authenticated publisher | `gate/auth.rs::AuthContext::attribution`, `gate/mod.rs:133-155,959-1003`, Scribe `execution_lanes.rs:1084-1154`; the derived identity and stamped columns use the same verified subject/current-actor pair | PASS |
| REQ-013, AC-009: managed flush precedes native drain, failures remain visible, active spans/provider survive | Python `otel.py:213-269`, native `state/mod.rs:279-320,518-544`; TS `index.ts:2727-2746,2963-2995`; shared `state.rs:2090-2163`; both runtimes settle OTEL failure after attempting native drain | PASS |
| INV-002: preserve existing Rust OTLP/log/metric and durable ownership | Shared trace/log/metric canonical dispatch call sites at Gate `:845,884,923`; native `dispatch_native_frame:1027-1105` retains supplied UUIDv7 ID, permission/Card checks, inline Scribe await and first-commit hook | PASS |
| Bifrost authority: batch identity, acknowledgment, recovery and query visibility | `bifrost-design.md` identity, append/staging/publication/recovery and outbox sections; `wyrd-design.md` identity/OTLP/managed lifecycle; telemetry, OLAP and analytical-reliability references | PASS |
| Existing memory and durable fences preserve exact accepted batch ownership | `preprocess.rs::LogicalIdentity`, `logical_data_identity`, `correlation_data_identity`; `memtable.rs::retained_batch_rows`; `shards.rs::write_group`, `batch_commit_fence`, `commit_batch_control_fences`, `resolve_replay_fence`, `resolve_replayed_commit`; SQL `scribe_batch_commits::{resolve,resolve_replay}` | PASS |
| Same authority's server-internal sibling remains distinct | `scribe_outbox.rs:786-888` hashes tenant/table/both attribution members/request/IPC; retryable refusals retain the slice, terminal refusals count loss. Gate's correction neither replaces nor modifies this owner | PASS |
| REQ-016 and AC-010: credible real readback, no duplicate logical spans | Python framework app `live:314-338` plus parent SDK readback; TS managed journey `:234-256`; OTLP attribution journey `negative.rs:390-554` and existing support/delegation helpers | PASS, execution limits below |

Repository rules, agent rules, task-review skill, spec-driven-development,
maintainer guide, reference router and doctrine were read. Governing authority
keeps durable behavior server-owned, rejects a new retry subsystem, distinguishes
WAL acknowledgement from publication and query visibility, and does not turn
OTEL force-flush into a lossless-delivery guarantee.

## Traced failure and recovery paths

1. **Current publisher collision closure (FIND-TASK-001-9).** The sole
   `otlp_batch_id` caller now passes `auth.attribution()`. Both pair members are
   hashed alongside unchanged tenant, table, logical and accepted correlation
   identities. Request IDs, ingest time, token bytes and historical delegation
   members do not affect that identity. Same-pair retries converge; a different
   actor no longer collapses onto the subject's accepted batch. Scribe continues
   excluding managed stamps from its logical digest; no downstream actor guard
   or alternate durable fence was added.
2. **Actual delegated readback (FIND-TASK-001-12).** The existing journey now
   reads `sub_principal_id` and `act_principal_id`, grouping by pair and Run.
   `token_with_permissions` seeds and bootstraps real services, and
   `WyrdTestServer::delegate:3498` exchanges real subject/actor tokens through
   `/auth/token` with Wyrd audience. Accepted exports cross HTTP protobuf and
   gRPC; repeated pair exports must produce five complete pair/Run sets, with
   every marker once. This is not a fabricated AuthContext or a rejected-token
   false proof. The two independent delegated actors and direct publishers are
   checked from stored rows.
3. **Flush outage/overlap.** Python keeps at most one active provider flush;
   callers enter under their own deadline and cannot treat an older running
   attempt as coverage of newly completed spans. The retained counter/Future
   makes a later call await a flush started after its entry. Timeout reports a
   catalog failure and still attempts native drain; the daemon may finish
   later. TS uses the installed processor's flush bound and catches rejection
   before native drain. Shared settlement retains native-only errors and the
   secondary native code when both stages fail. No durable queue or new retry
   policy was introduced.
4. **Failure before acknowledgement / interrupted process.** Stock OTEL queues
   remain runtime-owned and volatile; this change does not persist or replay
   them. Python's stock flush can return true after a refused export; the
   repository documents that limit and the negative journey checks persisted
   absence. A provider timeout does not cancel or claim successful completion
   of its running export. Native writer shutdown retains its existing retry-on-
   same-state behavior on drain failure. These are separate lifecycles.
5. **Acknowledged Scribe rows / restart.** The corrected derived UUID is passed
   unchanged into Scribe, whose memory index and SQL fence key remain tenant,
   table and batch. WAL replay reconstructs recorded identities rather than
   rerunning Gate's hash. Canonical WAL coordinates restore; later exact retry
   coordinates suppress; contradictory digest/count refuses recovery. Published
   ranges suppress already-published WAL before reconstruction. No R3 change
   affects those owners, the WAL format or retained staged files.
6. **Publication and query.** Test publication uses the existing
   `flush_bifrost`, followed by public Bifrost SDK query and terminal consumption.
   Successful state flush is not a promise of immediate Iceberg publication or
   a globally complete Oracle result. Current authority explicitly permits the
   published/live overlap tradeoff. The candidate adds no stronger query claim.

## Verification limits and rollout boundary

No source, index, build, install or test was changed/run by this reviewer.
Root-owned `verification.md` at inspection records fresh passing exact derived-ID
and settlement tests (one each), TS telemetry tests (five), optional-peer clean
package checks, and Python setup/callback/deadline tests (eight). Environment-
backed delegated attribution and actual framework/gateway journeys were still
running; this report does not count pending work as executed success. Committed
R3 evidence records the delegated actor mutation RED and journey GREEN; broader
historical evidence remains historical. The orchestrator must reconcile fresh
journey results before the task verdict.

The actor hash changes derived OTLP IDs even for direct publishers. An identical
export retried across old/new Gate deployments can therefore be accepted again;
old WAL data itself continues replaying its recorded ID. This is a concrete
rollout limit already recorded by R3, not a new persistent-format incompatibility.
The approved task does not authorize a migration or cross-version delivery
guarantee. No dual-fence lookup, migration, new retry registry or actor-specific
consumer guard is proposed. Same-version recovery semantics and native explicit
batch-ID semantics remain the inspected regression boundaries.

No new material durability finding or correction is proposed. Cross-version
retry deduplication and live crash/rolling-deployment experiments were not
certified; they are not claimed by this domain PASS.
