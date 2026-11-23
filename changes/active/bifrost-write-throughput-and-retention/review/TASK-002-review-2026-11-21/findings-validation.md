# TASK-002 independent validation

Immutable subject: base `5a057524f07d23a515d505caa4955539926d7fee` → candidate
`c40e7245c3ada4a35177e3a8c2d37c42d058c619`. Approved specification:
`SPEC-bifrost-write-throughput-and-retention` revision 3. Original task:
`tasks/TASK-002-cpu-throughput.md`. HEAD matched the candidate during validation.
No reviewed source was changed.

Read the approved requirements and task, applicable repository/workflow/rustdoc
rules, Bifrost resource and recovery authority, and all three discovery reports:
`task-review-behavior.md`, `task-review-invariants.md`, and
`task-review-domain.md`. Used CodeGraph before focused source inspection,
then inspected the cumulative diff, shutdown consumers, assertions, and the
specific source omitted from its excerpts. This validator did not author either
proposal and did not run Cargo concurrently with central verification.

The current maintainer explicitly approves the fork change and excludes missing
benchmarks as blockers. Performance qualification remains unmeasured; neither
that waiver nor this review establishes the throughput target or read tails.

## FIND-TASK-002-1 — terminal abort leaves encoder retention open

**CONFIRMED**, discovery ID `INV-1`, classification `INCORRECT`.

Requirement: REQ-012 failure/cancellation cleanup, original scenario 3, and
`architecture/bifrost-design.md` resource/failure invariants requiring structured
cancellation and release of ownership. This is the newly introduced encoder
owner's terminal lifecycle, not a demand that all unrelated Scribe state become
empty at shutdown.

Location: `crates/vala/vala-bifrost-redux/src/scribe/mod.rs:1453`,
`:1488–1533`; retained owner `scribe/preprocess.rs:1009–1031` and
`scribe/execution_lanes.rs:177–220`.

A successful append warms an ingress worker's retained native compressor context.
The caller then directly aborts Scribe, or cancels graceful shutdown before its
context-clear continuation. `ShutdownCancellationFinalizer::drop` explicitly
calls `abort_shutdown` (`mod.rs:374–379`); the real server deadline-expiry path
also invokes it (`wyrd-server/src/state.rs:823–828`). These are supported paths,
not hypothetical access to a private helper.

Expected: terminal closure releases idle reusable contexts and prevents later
restoration. A synchronous encoder already running must retain its lease until
it actually finishes, then release rather than refill a stopped lane.

Actual: only the ordinary `shutdown` continuation calls `encoders().clear()`.
`abort_shutdown` calls `begin_shutdown`, closes lane permits, and calls the common
finalizer, which publishes STOPPED without closing the encoder set. The Scribe
still retains its ingress pool and `Arc<EncoderContexts>`; `close` only closes
its semaphore. Thus the native context and root Metadata lease remain live,
and the still-open set permits an in-flight encode to restore. Subsequent
`shutdown` returns through its already-STOPPED branch without clearing it.

Independent validation traced both terminal callers, the retained Arc field,
`EncoderContexts::restore`'s closed checks, and the absence of any equivalent
clear in the common finalizer. This establishes the ownership outcome directly.
The existing cancellation test at `scribe/tests/wal_closeout.rs:458` starts
without warming an encoder and checks other categories, so its green execution
does not disprove this case.

Consequence: stopped Scribe handles retain unusable, bounded encoder memory and
its shared-root charge until the last owner drops. No unbounded growth, data
loss, or effect after process exit is claimed.

Smallest correction: use the existing context-set closure at the shared terminal
shutdown owner so all terminal paths close retention. Preserve the current
clear/restore race protection and live worker lease lifetime; do not add waits,
workers, or detached cleanup.

Closure proof: in the existing inline shutdown lifecycle tests, warm actual
retention and assert it is positive, retain the Scribe handle, then separately
exercise direct abort and cancelled graceful shutdown. Idle encoder retention
must become zero; after an already-running encode settles, it must not restore
into the stopped set. Keep live work charged until settlement. This internal
lifecycle assertion belongs at the owner tier and needs no new wire surface.

## FIND-TASK-002-2 — captured-source rustdoc attaches to the wrong type

**CONFIRMED**, discovery ID `B1`, classification `VIOLATION`.

Authority: root `AGENTS.md` requires rustdoc on every new/changed Rust item and
explicitly says missing rustdoc blocks merge.

Location: `crates/vala/vala-bifrost-redux/src/scribe/memtable.rs:2221–2228`
and `:2313–2314`.

The inserted `RecordedBounds` type follows the original six-line documentation
for `ReadableSource`. Rust attaches that description to the adapter, followed by
the adapter's new documentation; `ReadableSource` now has no item documentation.
The cumulative diff and final source independently establish the displacement.
Expected: the existing capture ownership/lifetime explanation remains attached
to its source owner, while the adapter retains its own accurate explanation.

Consequence: the change removes required owner documentation and attaches its
ownership contract to the wrong item. This is an explicit mandatory-rule
violation, not a discretionary preference about prose.

Smallest correction and closure: move the original capture description directly
above `ReadableSource`, leaving the recorded-bounds description on the adapter.
Source inspection plus formatting is sufficient; no runtime test or surrounding
documentation rewrite is required.

## FIND-TASK-002-3 — shipped-feature Clippy gate fails on unused constructor

**CONFIRMED**, independently checked after central gate execution, classification
`VIOLATION`.

Authority: original task final verification explicitly requires `mise run lints`;
root `AGENTS.md` requires required checks and forbids bypassing a red gate.
`mise.toml:55–60` deliberately checks both the workspace feature union and the
shipped server feature set.

Location: `crates/vala/vala-bifrost-redux/src/scribe/wal.rs:511`.

Trigger: run `mise run lints` against the candidate. Its first workspace
all-features/all-targets Clippy command passes. Its second command,
`cargo clippy --locked -p wyrd-server --bin wyrd-server -- -D warnings`, exits
101 because `PreparedWalAppend::new` is dead code under shipped features.
This execution result was obtained by the central runner; this validator
independently inspected the declaration, cumulative diff, all named callers,
and their compilation conditions.

The production preprocess now uses `PreparedWalAppend::chunked`. Remaining
`new` callers are compiled only for tests, `test-support`, or `bench-support`:
WAL replay helpers at `wal.rs:2539` and `:2837`, test-only WAL/replay assertions,
and `bench_support.rs:152` whose module is feature-gated in `lib.rs:10`.
The constructor itself remains unconditional, explaining why the all-feature
check passes while the shipped feature check fails.

Expected: the mandatory shipped-feature lint check passes without warning
suppression. Actual: the required lint gate fails. This is a verified gate
violation; no runtime WAL corruption is alleged.

Smallest correction boundary: remove the now-unused production convenience
constructor or restrict it to the existing test/benchmark consumers' supported
compilation boundary, preserving their ability to construct test payloads and
the production chunked path. Do not add a feature or silence dead-code warnings.
Run `mise run lints` to prove both feature shapes; retain the focused WAL chunk
and replay helper coverage if call sites change. No new runtime test is needed
for the compiler-detected property.

## Reconciled coverage and proof strength

The verdict owns the final requirement/scenario matrix. Its source-backed rows
should record REQ-001 and REQ-004 preservation as PASS; REQ-010 and REQ-011 as
PASS; REQ-012 as FAIL through FIND-TASK-002-1; REQ-013 read algorithm preservation
as supported by the inspected owner assertions, with the two focused Oracle
journey results added by the central runner. REQ-013 performance is UNRESOLVED
and expressly waived as a blocker. Mandatory rustdoc is FAIL through
FIND-TASK-002-2. Format and dependency checks PASS. Workspace Clippy PASS, but
the shipped-feature Clippy check FAILS through FIND-TASK-002-3.

The domain report traces segmented producer output through actual framing,
ordered digest/CRC, bounded short-write windows, rollback and accepted-only
replay, and checks the downstream assertion consumers. No production flattening,
extra durability group, or changed fence authority was found. The invariant
report establishes bounded root-backed retention, independent stream state,
worker isolation and actual allocation accounting apart from its terminal-path
finding. Primary review covers checked projector reservation, shard control and
pre-ACK publications, conservative projected-to-source statistics, identity
second-pass intersection, optimized-plan authorization, registered physical
planning, coherent Arrow pins, and remaining repository rules. Independent
validation inspected the affected diff and source for these seams and found no
additional material blocker.

Specialist selection is sufficient: the invariant reviewer covers encoder
concurrency/accounting/cancellation; the domain reviewer covers persistent bytes,
IO failure and recovery; primary covers read authorization and general rules.
An additional system, broad repository, or maintainer reviewer would duplicate
these bounded assignments. There is no new SQL schema, cross-tier public
contract, SDK surface, or resource scheduler needing separate expertise.

Task-recorded evidence is candidate-bound for 993 redux library tests, 75
Scribe/OTLP journeys and crate Clippy; earlier integration evidence predates
final reuse and is supplemented by those later executions and the new focused
context assertions. Fork tests compare independent streams and levels. Final
central checks and Oracle journey outcomes are reported by the orchestrator;
this document does not claim their unreported results.

No new/relocated PostgreSQL, object-store, Vault, or Scribe wait was identified.
Vectored syscall windows stay inside the existing sanctioned append operation;
physical planning uses the existing registered planner and storage-read boundary.

TDD compliance: PASS as recorded, with no independently demonstrated violation.
Historical order is not fully reconstructible from bundled commits; in
particular the scenario-3 stubbed RED could be test-first scaffolding or a later
mutation. Do not claim it independently proves historical order, and do not
require fabricated historical RED or production churn. Behavior is judged by
actual assertions and source separately.

Recommended verdict: **FIX_REQUIRED**, retaining only the three findings above.
One cohesive remediation task can close terminal encoder retention, restore
the displaced documentation, and fix the constructor compilation boundary;
no redesign, fork reversal, or benchmark run is
required by these findings.

Skipped: duplicate Cargo runs and benchmarks. Performance gains remain
unmeasured; runtime reproduction of FIND-TASK-002-1 is a closure test for the
implementer, while its control-flow and ownership defect is source-proven.
