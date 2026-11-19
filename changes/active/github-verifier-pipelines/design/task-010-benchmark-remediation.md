# Restore resource-scaling direct verification

Status: finalized — maintainer agreed; L1-only qualification

The maintainer agreed this recommendation and requested
[TASK-010-R1](../tasks/010-R1-restore-direct-verification-baseline.md).
Measurement is limited to L1 verification, gateway and Bifrost. Agreement fixes
the design; it does not claim implementation or benchmark success.

## Problem and outcome

The packet's candidate `b709eaa21` sheds writes and refuses nearly all requests
in its verification L1 64-client cases. The clean resource-scaling reference
has neither loss nor refusals. The maintainer requires no regression in
verification, gateway or Bifrost benchmarks, regardless of added functionality.
Direct verification must align with resource-scaling; resumable execution state
belongs to queued verification. Both use the same engine. Persistence uses
batching and group commit.

## Verified baseline

An independent researcher inspected the actual resource-scaling worktree at
`/home/thorrester/Documents/GitHub/wyrd-worktrees/resource-scaling`, HEAD
`443a79337`, and compared the relevant functions with clean benchmark source
`2ce7caec7`.

Direct reserves two outbox slots, executes the engine, and stages two items
projected into three existing Bifrost records:

| Record | Content |
|---|---|
| `execution_inputs` | Supplied declarations, `origin=direct`, `queue_execution=false` |
| `execution_outputs` | Requirements and step details |
| `results` | Compact verdict, status, timing and identity summary |

Normal authorization audit and model gateway capture remain with their existing
owners. Direct creates no `RunState`, checkpoint/history record or SQL execution
row. Background delivery batches records by destination.

The baseline prepares JSON/Variant rows before responding. Arrow construction,
durable delivery and acknowledgement happen in background. Restoring baseline
does not imply every allocation or serialization was already off the hot path.

Baseline references under `crates/wyrd/wyrd-server/src/` in that worktree:
`components/verification/service.rs::run_direct` (605), `stage_direct` (886),
`verification/results.rs::build` (290), `inputs` (365), and
`scribe_outbox.rs::frames` / `Frame::absorb` (1302).

## Diagnosis and evidence

Candidate record:
`benchmarks/history/verification/direct-verify/20261112T190855Z-b709eaa21.json`.
Baseline: `verification/direct-verify/20261112T030209Z-2ce7caec7.json` at
benchmark-records commit `93fd0f240`; matching host, build, fixture, limits and
method. Existing logs/snapshots were inspected; no new benchmark ran.

| Observation | Consequence |
|---|---|
| Baseline c1 median 11,654 verifications/s, 110 microseconds CPU/verification, zero loss | Clean reference |
| Candidate c1 median 6,210/s, 520 microseconds CPU/verification; 52,845 / 54,979 / 67,423 shed writes | Integrity failure and increased CPU cost; INVALID measurements cannot qualify performance |
| Candidate c64 successes 6,813 / 0 / 9,219; refusals 547,569 / 554,907 / 530,967 | Severe refusal regression; only repetition 2 refuses every call |
| First staging refusal: 805,314,997 admitted bytes against 805,306,368 limit | Reserved byte budget saturated |
| c64 repetition 2: 9,640 Scribe frames finish, zero outbox slices finish; pending remains 303,466 | Delivery progresses but slice-held admission capacity cannot return |

The packet adds independently keyed checkpoint frames and also changes every
result, including direct results, into separate output and summary frames.
Resource-scaling grouped direct results by destination. With eight frames in
flight per slice, many singleton frames require many acknowledgement rounds.
Admission capacity returns only when the whole slice finishes. Direct background
state production also sheds on unreserved staging and abandons that run's
subsequent persistence.

Sources: [state writer](../../../../crates/wyrd/wyrd-server/src/verification/state.rs),
[frame construction/delivery](../../../../crates/wyrd/wyrd-server/src/scribe_outbox.rs),
[slice capacity lifetime](../../../../crates/shared/wyrd-runtime/src/outbox.rs).
Raw evidence:
`target/benchmarks/verification/direct-verify/server.log` and
`target/benchmarks/verification/direct-verify/20261112T190855Z/timings/`.

All measured loss increments are `shed`. The counter includes audit events
from refused requests, so it is not a count of missing verification results.
Logs independently prove abandoned state persistence. Snapshot preparation,
encoding, logging and durable-write CPU costs have not been separately measured.
The evidence establishes write amplification and retained-capacity backlog;
it does not prove the proposed repair alone restores all benchmark results.

## Proposed flow and implementation

### How engine additions remain supported

There is one graph engine, with the same step scheduling, Agent/code execution,
dependencies, limits and live outcomes for both entry paths. The queued runner
adds the existing optional checkpoint adapter to record progress and restore
accepted work. The direct caller does not attach it. This distinction already
exists in `VerificationEngine::new` and `with_checkpoints`; the proposal uses it
rather than creating another execution implementation.

| Capability | Direct | Queued |
|---|---|---|
| Ordinary eligible graph, Agent/code work, live dependencies and limits | Shared engine; existing result recording | Same engine, plus checkpoint adapter |
| PR extraction, deferred input and approval | Existing eligibility validation refuses before work | Existing queued feature flow; park/release/resume as applicable |
| Recovery after process death, accepted invocation/publication replay, retained execution history | No resumable execution state | Existing accepted state and queue/lease fences |
| Result and details retrieval | Existing input/output/result records | Existing result records and accepted execution state as appropriate |

The first two rows preserve the shared-engine and existing queued-only feature
boundaries. The maintainer also agreed the researcher's consumer-closure proposal
to keep newly added hosted publication receipts and execution-history features
queued-only. Ordinary direct graph/tool/file behavior remains supported. This
agreed public capability boundary requires the authority reconciliation below.

**Direct:** request/declarations → existing validation, authorization, target
preparation and two-slot admission → shared engine without checkpoint adapter →
normal owned outcome → existing input/output/summary projection → process Scribe
outbox → destination-batched background delivery → normal result/details reads.

`VerificationEngine::new` already has `checkpoints: None`; `with_checkpoints`
attaches the optional queued adapter. Remove direct `RunState` construction,
adapter attachment and state-based terminal selection/failure. Assign the normal
result identity once through the existing result projection. Preserve transient
engine scheduling, retries, budgets and outcome values. No second engine or
resumable state under a different direct type.

Restore `Frame::absorb` destination grouping for direct results. Preserve queued
result/revision replay keys and acknowledgement semantics, slice retry identity,
detail-before-summary ordering, attribution and tenant isolation. Removing direct
checkpoints alone leaves result-frame amplification intact. More buffer space,
more writers or smaller slices do not replace the baseline batching workflow.

**Queued:** existing batched admission/claims → prepared `RunState` and checkpoint
adapter on the same engine → memory progress and coalesced background checkpoints →
release compute at terminal selection → bounded claim custody/renewal → accepted
terminal state/results → token-fenced batched settlement.

Keep exact accepted recovery, conflict/replay identities, cancellation and
lease-loss cleanup. Never settle a shed terminal offer. Preserve existing Scribe
shard WAL group commit and tenant-batched fence inserts, and set-based PostgreSQL
admission/claim/renewal/completion. Independently fenced queued revisions are
logical batch identities; retain physical group commit. Low traffic can yield
one-item batches, but do not construct a durable transaction per record.

Accepted-revision readers probe deterministic fences and load that exact state.
Do not replace them with `MAX(sequence)` or an unfenced analytical read. No
Bifrost WAL/catalog redesign, multi-identity envelope, new queue/finalizer,
dependency or feature is selected.

**Consumers:** restore direct status/details lookup through the actual baseline
input/output/result records instead of `RunState::accepted`. Keep current
`verifier:read`, authorization/audit, tenancy and explicit missing/unavailable
details behavior. Queued inspection continues through accepted state.

New hosted publication receipts, workspace mounts and execution-history readers
currently depend on `RunState` through `HostedTools` and `TenantEvidence`.
Resource-scaling had no direct hosted publication backend. The proposal keeps
these resumable features queued-only, with existing queued-only validation before
execution; it does not remove ordinary baseline direct tools/file behavior or
recreate publication custody under another type. Record this public capability
boundary explicitly in the spec amendment before implementation.

Affected owners include `components/verification/service{,/inspection}.rs`,
`verification/{state,results,inputs}.rs`, `verification/hosted/`,
`vala-verify/src/blocks/engine.rs`, `scribe_outbox.rs` and their existing journeys.
Paths guide complete consumer closure rather than impose a file allowlist.

## Authority and handoff

The maintainer selected baseline direct recording, queued-only resumable state,
one engine, batching/group commit and no regression across all three engines.
The maintainer agreed the complete recommendation and selected L1-only measurement.

Before implementation, amend the direct `RunState`/checkpoint and inspection
requirements in spec revision 7, `contracts.md`, task 010, its nonblocking
supplement, and the owning verification sections of `architecture/wyrd-design.md`
and `architecture/bifrost-design.md`. Document direct result-record inspection
versus queued history/publication. Preserve queued custody and accepted recovery.
No authority was silently edited to justify the candidate.

No final direct snapshot, direct state archive, result mirror, stronger crash
guarantee or restored synchronous checkpoint wait is selected. Moving all baseline
row preparation off the response path is not an additional selected optimization.

## Connected proof and acceptance

- Real SDK → server/Postgres → SDK direct result/details: baseline records and
  attribution remain, no direct state/checkpoint or SQL execution row, and held
  persistence acknowledgement does not hold computation.
- Frame tests: many direct results become destination batches; retry identity,
  detail-before-summary ordering and queued acknowledgement/replay remain intact.
- Existing queued recovery, conflict, cancellation, lease-loss and terminal-custody
  journeys: accepted work recovers exactly; queued acknowledgements held through
  claim-tracking exhaustion leave running work able to finish, compute free and
  claims/bytes bounded, and pause new claims. Direct work still completes while
  its delivery remains available. Restoration leads to batched fenced settlement
  of the same terminal evidence without recomputation and releases capacity.
  Direct and queued readers consume their actual recording paths.
- Queued-only direct eligibility is checked before graph execution, including
  declared Agent `publish` tools and publication-dependent mounts. Refusal must
  precede any earlier observable step; ordinary baseline direct tools/input mounts
  remain supported. Do not defer refusal until a tool session lacks `RunState`.

All five L1 experiment records must measure one final immutable candidate source
revision. If production/test source changes after measurement, rerun all three
suites on that final revision rather than mixing pre-fix and post-fix records.
Qualify that candidate against the newest comparable clean
resource-scaling records for each experiment on the same quiet host, with matched
build/features/limits/fixtures/rates. Retain source IDs and all repetitions. Missing
comparable evidence requires an isolated baseline run, not a degraded packet
candidate promoted as baseline. Run full default coverage sequentially:

```bash
mise run bench:l1:verification
mise run bench:l1:gateway
mise run bench:l1:bifrost
```

Bifrost L1 requires all three experiment records. Smoke, subset, profile and
dirty diagnostics cannot replace clean qualification. Compare latency, throughput,
CPU efficiency, peak memory, correctness, retained evidence and each L1
experiment's existing integrity/settlement criteria. Any regression fails the whole packet. INVALID,
unsettled or missing evidence is not PASS; VALID/exit zero alone does not certify
performance. No invented tolerance, weakened counter, reduced workload or
baseline reset is authorized.

L2 measurement is excluded by the maintainer's latest instruction; it is not
required to accept this remediation. Queued correctness/recovery tests remain
required. No production edits, benchmark runs or database changes were made in
this research. Performance restoration remains to be demonstrated.
