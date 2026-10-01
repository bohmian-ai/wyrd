# Independent behavior review — TASK-008-R5

Result: **FAIL**, for one bounded leftover-reference correction. FIND-007-10, FIND-007-11, and FIND-007-12 are closed. No runtime durability, replay, restart-publication, tenant-isolation, or transport regression was established.

## Subject and limits

Candidate `1a66d8a7b4583c9798c2f1573dc6ab1f3eadb027`; parent `ca99db0af5a0d898ef67834699405c1c73719f56`; original TASK-007 base `a7582db587c6170a290760f1741673125612b797`; TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. Reviewed the approved revision-20 specification, original TASK-007/TASK-008, prior R4 verdict/remediation and appended R5 evidence, cumulative changed-surface map and actual source/diffs at the acceptance and recovery seams.

Authorities: AGENTS.md, agent-rules.md, spec-driven-development.md, maintainer-style.md, Wyrd design/doctrine and applicable Bifrost scan, durability, shutdown, tenancy and recovery authority. No CodeGraph directory is present. Static inspection only; no cargo, nextest, mise, build, test, benchmark or commit. Verification below is supplied evidence, not reviewer execution. This is the requested confirmation scope, not a new capacity qualification or reconsideration of approved decisions.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-014: staged files reuse the shared scan and pruning; bespoke staged decoder removed | `oracle/follower.rs::ScribeTailResolver::live_leaf` builds `HotParquetExec`; shared `oracle/exec.rs` selects row groups/pages and projection before decode; `scribe/staged_tail.rs` deleted | Original focused pruning and lease tests; prior module/journey results | PASS |
| Memory rows use engine source and signed predicate | `live_leaf` creates `MemorySourceConfig` at the closure schema and applies one signed predicate above memory/staged union | Focused nonzero-ordinal projection/filter evidence | PASS |
| Session partitions, Scribe-local IO and grant ownership | Follower takes `session.config().target_partitions()`; staged IO exists in local resolver; hot scan receives follower memory pool and staged lease | Existing follower partition/lease evidence and journeys | PASS |
| Native in-process Arrow, unchanged remote footer behavior | `LiveFrame::{Batch,Complete}` and `LiveFrameDecoder::accept` retain counts/fingerprint validation; wire branch retains schema/IPC/hash/footer validation | Existing live decoder and remote peer tests | PASS |
| REQ-015: remove analytical row tenant field/filter/codec; authenticated footer producers | Managed columns, writer and scan projections omit row field; writer tenant comes from seal/table binding; assembly carries tenant and Forge writer properties stamp it; old provider/filter/tripwire removed | Supplied focused, schema, codegen and tenant-isolation evidence | PASS |
| Fresh and cached footer proof before any row, including aggregate reads | `PublishedFooterLoader::load` proves cached/loaded metadata before returning it; `tenant_proven_reader_metadata` proves hot/staged metadata before creating stream builder; missing/duplicate/foreign proof refuses | Focused footer negatives and local/distributed aggregate journey evidence | PASS |
| Remote tenant refusal remains terminal/security, not optional source loss | `execution_status_error` and `stream_status_error` retain `Aborted -> TenantInvariant`; leader public mapping and first-refusal audit remain | R5 classifier 1/1 and remote-footer journey in focused 3/3 | PASS |
| FIND-007-10: Scribe-containing listener stops blocked accepted IO | `app/server.rs:681–690` selects stopping runner when `bifrost.scribe().is_some()`; IO cancellation future wakes connection driver independently of response-body pulls | R4 RED/GREEN; R5 blocked-window/lost-Scribe focused passes; peer 11/11 | PASS / CLOSED |
| FIND-007-10: Oracle-only/public graceful semantics preserved | Same listener selection uses original `serve_grpc_with_listener` for absent Scribe; `build_peer_grpc` mounts Scribe services from the same actual capability; public listener remains original runner | Static restoration of original runner; supplied peer lane; no new Oracle-only journey claimed | PASS / CLOSED |
| FIND-007-11: actual IO Errors and ConnectInfo documented | Five stopping IO methods document cancellation versus socket errors; shutdown delegates after cancellation; TCP connect info is explicitly documented | Source inspection and supplied lints | PASS / CLOSED |
| FIND-007-12: exact named focused recipes/results | Appended R5 evidence records explicit package/target/exact selectors, Postgres wrapper and result counts for classifier, remote footer, lost Scribe and blocked window | Supplied 1/1 and 3/3, honest R4 RED attribution | PASS / CLOSED |
| Approved shutdown decision: leave residue staged, retain ACK/WAL durability and replay | `ScribeImpl::shutdown` still flushes shards and drains accepted persistence before closing lanes; residue sweep removed; no durable-file deletion added; startup restores stage then reconciles/resumes claims before WAL replay | Supplied redux integration 872/872; Scribe journey 20/20; server journey 26/26; clean 0.6-second heavy stop | PASS |
| Restart publishes restored ready members on normal tick | `StagingRuntime::restore` rebuilds ready/outstanding ownership and encoding context; ready member age survives recovery; `StagingAssembler::due_key_for` selects target/dwell; `app/server.rs:557–575` lifecycle scanner calls `publish_due` without new writes | Source trace plus supplied integration/journeys; existing idle publication and restart/exact-read tests are complementary, not a newly run combined scenario | PASS |
| Leftover workflow references describe new shutdown semantics accurately | Assembly ready-key docs, persistence worker docs and lifecycle journey docs still promise shutdown residue publication | Static source contradiction, BEH-R5-1 below | FAIL |
| Fixed decisions/non-goals | No live-read cap or Postgres-column scope expansion; established public error and FIND-007-3 disposition preserved; shutdown policy accepted | Static inspection; N/A runtime | PASS |

## Caller and recovery traces

For a query-only peer pod, `build_peer_grpc` creates Oracle/Analytical/lifecycle services without a Scribe tail service. The listener owner now consults the actual retained Scribe capability and selects the original graceful runner. It does not insert a downstream guard or alter Analytical lease settlement. A mixed or Scribe-only pod retains stopping IO, and forwarded `TcpConnectInfo` remains the value tonic augments with TLS peer identity. Normal reads/writes delegate until shutdown; write-half shutdown delegates even after cancellation. This closes the original producer-side listener-selection failure while retaining blocked-Scribe cleanup.

For an acknowledged write followed by graceful stop, final shard rotation still submits the generation and persistence drain still waits for its durable staged ownership before execution lanes close. The removed sweep is a publication step after that boundary, not the ACK or fsync step. Retained WAL and staged member records remain authoritative. At restart, `replay_wal_async` calls restore, publication reconciliation and interrupted-claim resumption before sequential WAL replay. `restore` reconstructs member ownership from stage facts, filters already-terminal claims and preserves claim identity; ready members enter the assembler's existing ready index. The normal supervised clock calls `publish_due`; due selection uses the recovered members' original ready times. Removal of the shutdown sweep therefore does not remove the restart route to publication or introduce a second batch identity.

Explicit `flush_staged` remains different: it deliberately drains accepted persistence and calls `publish_residue(ClaimCause::Drain)` before retirement. Keeping the method and enum value is required by that reachable caller. They are not executable leftover shutdown publication. Existing lifecycle test assertions permit either zero or complete publication after restart and assert exact public reads, then explicitly flush recovered rows; its stale prose is the defect, not its all-or-none fence assertion.

## Proposed finding

### BEH-R5-1 — INCORRECT: current workflow documentation still promises the deleted shutdown sweep

**Obligation:** User specifically requested leftover-reference checking for the approved shutdown decision; `architecture/bifrost-design.md:323–327` now requires staged residue to survive shutdown for the next process, and repository rustdoc must describe actual workflow/invariants.

**Locations:**

- `crates/vala/vala-bifrost-redux/src/scribe/assembly.rs:982–985`: `ready_keys` says every remaining key is claimed as residue at shutdown.
- `crates/vala/vala-bifrost-redux/src/scribe/assembly.rs:437`: `ClaimCause::Drain` describes settlement before shutdown, although its live caller is explicit flush.
- `crates/vala/vala-bifrost-redux/src/scribe/persistence.rs:575`: retained worker is described as enabling drain residue publication.
- `crates/wyrd/wyrd-testing/tests/bifrost/scribe/lifecycle.rs:106–110,122–124,149`: the real-server lifecycle journey says graceful stop sweeps every staged member, leaves nothing for restart and owes publication.

**Evidence and consequence:** `ScribeImpl::shutdown` no longer calls residue publication; the sole normal caller is `flush_staged`. These comments now contradict both the owning design and actual current path, and misdescribe what the real shutdown/restart journey proves. A maintainer consulting the owner/test would infer that a clean stop settles staging when retained ready members are now expected. No lost-row or replay defect is claimed.

**Smallest correction:** Update only these workflow descriptions to distinguish explicit flush from bounded shutdown staging/recovery. Reuse the accurate shutdown and `PersistenceWorker::publish_residue` descriptions already changed in this candidate. Preserve `ClaimCause::Drain`, explicit residue flush, tests/assertions and all runtime behavior. Do not restore a shutdown sweep or invent new lifecycle machinery.

**Focused proof:** Static search of current Scribe shutdown/residue descriptions and source caller tracing confirms no current documentation promises a shutdown residue sweep; explicit flush docs continue describing their real caller. No new runtime test is needed for a prose-only correction.

## Overall result

**FAIL — BEH-R5-1 only.** All three requested prior findings close. Shutdown policy is accepted, with no demonstrated runtime correctness regression. The bounded proposal concerns remaining contradictory current references introduced by deleting the sweep.
