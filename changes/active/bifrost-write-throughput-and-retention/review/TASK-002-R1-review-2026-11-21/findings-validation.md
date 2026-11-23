# TASK-002-R1 independent remediation validation

Recommended verdict: **PASS**. All three prior findings are closed; no new
blocking finding was found in the remediation or affected paths.

## Immutable subject and scope

- Original task base: `5a057524f07d23a515d505caa4955539926d7fee`.
- Prior candidate: `c40e7245c3ada4a35177e3a8c2d37c42d058c619`.
- Remediated candidate: `702629be40e33f3decb34b4db73eff6846a55132`.
- Approved contract: [spec revision 3](../../spec.md).
- Original task: [TASK-002](../../tasks/TASK-002-cpu-throughput.md).
- Prior verdict: [first review](../TASK-002-review-2026-11-21/verdict.md).
- Remediation: [TASK-002-R1](../TASK-002-review-2026-11-21/TASK-002-R1-terminal-encoder-cleanup.md).

Reviewed `git diff c40e7245c 702629be4` and its affected consumers, retaining
the original cumulative range as context. HEAD matched the candidate during
source review and at completion. No reviewed source or prior report was edited.

This fresh reviewer did not discover the original findings or implement their
fixes. Read root AGENTS.md, the task-review skill, applicable spec requirements,
the remediation's scenarios/evidence, prior findings/verdict, workflow remediation
and TDD rules, maintainer style, Rust guidance, TESTING.md, and Bifrost shutdown
and resource/failure authority. No nested AGENTS applies under the changed Vala
tree. CodeGraph was used before focused source tracing; excerpts that did not
cover the needed fixture and caller conditions were followed by bounded reads.

## Prior finding closure

| Finding | Independent source and proof assessment | Closure |
|---|---|---|
| FIND-TASK-002-1 / REQ-012 | `scribe/mod.rs:1517–1538` clears the existing context set in the shared finalizer before publishing STOPPED. Ordinary shutdown reaches that owner at line 1462; direct abort reaches it at 1507; the cancellation guard at 374–379 invokes abort. The server's existing deadline abort uses this same path. `preprocess.rs:925–964` retains the taken lease locally through synchronous encoding; `restore` checks closure before retaining and again under the slot lock. Clearing drops idle contexts without touching a context already taken by an encoder. | CLOSED |
| FIND-TASK-002-2 / mandatory rustdoc | `memtable.rs:2307–2314` again attaches the six-line capture ownership description directly to `ReadableSource`. `RecordedBounds` retains its own description at 2216 onward. The change is only a documentation move; no pruning behavior changes. | CLOSED |
| FIND-TASK-002-3 / mandatory lint gate | `wal.rs:31` and `515` gate both `Bytes` and the convenience constructor to `any(test, test-support, bench-support)`. Remaining callers are test modules, test-support replay helpers, or the bench-support module; production still uses `chunked`. Existing feature boundaries are reused without new features or warning suppression. Both commands in the mandatory lint task now pass. | CLOSED |

The direct-abort regression (`wal_closeout.rs:506`) warms retention through
the real `Scribe::ingest_frame` preparation/durable seam, observes a positive
encoder charge, retains the handle, calls abort, and checks both zero encoder
retention and the exact Metadata reduction. Repeated shutdown/abort remains
empty. The cancellation regression (`wal_closeout.rs:529`) warms first, stalls
the existing shutdown owner, cancels it, and checks release immediately after
the finalizer, before subsequent shutdown could mask missing cleanup.

The owner-tier `closure_during_encode_keeps_its_lease_until_settlement`
(`preprocess.rs:1678`) first encodes real rows and compares bytes with an
independent writer. It then deterministically takes the actual retained context
and lease out of the slot, exactly matching `encode`'s ownership state, closes
the set, and asserts the root stays charged. Completing through real `restore`
or dropping on failure releases the charge and leaves the slot empty. This is
an owner-state test, not an execution of a literally concurrent encoder; its
comments and task evidence state that limit. Combined with the unchanged,
directly inspected production take/encode/restore path, it supplies credible
proof of the required lease and no-restore invariant without timing machinery.

## Coverage carried forward and affected seams

The verdict owns the final requirement matrix. REQ-001, REQ-004, REQ-010,
REQ-011 and the functional read-side part of REQ-013 retain their previous
source-backed PASS evidence. This remediation changes no dependency pin,
authority, input decoder/projector, logical/physical query planner, IPC encoding
settings, WAL bytes, acceptance fence, or replay rule. No specific regression
or invalidated proof justifies reopening those areas. The earlier review's
focused Oracle journeys and final Scribe/OTLP journeys therefore remain usable.
REQ-012 now combines its prior bounded-memory/stream-isolation evidence with
the closed terminal lifecycle finding above.

No new or relocated PostgreSQL, Scribe, object-storage, or Vault wait is added.
The existing bounded slot closure runs in the terminal owner; it introduces no
async cleanup, worker, executor, external IO, or durability/ACK-path operation.
Graceful shutdown still drains before finalization; abort still skips graceful
flush and leaves unfinished durable work to existing WAL recovery.

The default single fresh remediation reviewer is sufficient. The changes are
one existing closure call moved to its common owner, owner-level regression
tests, a documentation move, and compilation scoping to existing consumers.
They introduce no new persistent or public boundary requiring fresh discovery,
domain, system, repo, or maintainer specialists. Original specialist evidence
continues to cover unchanged persistence/accounting/read boundaries. No new
finding needs a second independent source-validation pass.

## Executable evidence and TDD

Fresh central execution reported by the orchestrator on the immutable candidate:

- Exact five focused C1+C2 tests: **5/5 PASS**, 990 skipped, nextest run
  `83406ad9-9f7f-4e35-8a7a-05a3751b4391`. These are the new direct-abort test,
  extended cancellation test, existing duplicate-shutdown and reuse tests,
  and the new closure/lease owner test. Actual module path is
  `scribe::wal_closeout`, correcting the stale selector in the original review.
- `mise run fmt:check`: **PASS**.
- `mise run check:deps`: **PASS**, including its one inline-security gate test,
  nextest run `c0252909-dc84-4276-8cd9-c197122db5c2`.
- `mise run lints`: **PASS**, both workspace all-features/all-targets and the
  separately checked shipped-server feature graph.
- `git diff --check c40e7245c 702629be4`: **PASS**.

The remediation additionally records 443/443 Scribe library regressions under
repository Postgres and Clippy with `test-support` alone and `bench-support`
alone. These are recorded implementation evidence, not additional runs by this
reviewer; the actual feature conditions, assertions, and connected owner paths
were independently inspected. Cargo execution remained serial with the root's
checks. There is no need to rerun broad journey suites for this bounded internal
cleanup correction.

Behavioral verification: **PASS**. TDD compliance: **PASS as recorded**, with
no contrary evidence. The task records both warmed terminal tests failing with
26,327 retained bytes before moving the production clear, then passing. The
lease/restore behavior already existed, so its later regression proof correctly
has no manufactured historical RED; the recorded mutation only demonstrates
assertion strength. Documentation and cfg closure use structural/compiler proof.
As in the original verdict, bundled commits alone do not independently certify
all historical execution ordering; no process violation is inferred from that.

The maintainer's explicit fork approval and benchmark deferral remain effective.
REQ-013 throughput, workload-matrix and tail qualification stays **UNRESOLVED,
nonblocking by instruction**. PASS here does not claim those measurements exist.
This first remediation round closes all retained findings; no convergence issue
or further remediation task is needed.

Skipped: duplicate Cargo execution, full journey suites, and benchmarks.
Throughput gains and read/mixed-load tails remain unmeasured.
