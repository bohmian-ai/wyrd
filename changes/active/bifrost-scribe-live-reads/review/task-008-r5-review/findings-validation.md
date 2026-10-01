# Independent structured Ponytail validation — TASK-008-R5

**Result: one REVISED, deduplicated finding, FIND-007-13. FIND-007-10, FIND-007-11 and FIND-007-12 are CLOSED. No executable shutdown correctness regression is established.**

## Immutable subject and authority

Candidate `1a66d8a7b4583c9798c2f1573dc6ab1f3eadb027`; correction parent `ca99db0af5a0d898ef67834699405c1c73719f56`; cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`; TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. HEAD was independently checked and remains the candidate. Inputs include the complete supplied cumulative/correction diff, approved revision-20 specification, original TASK-007/TASK-008, prior R4 verdict/validation and R5 task/evidence, all eight discovery reports, claim comparison and focused follow-up. Source and caller evidence below was inspected independently rather than accepted because reviewers agreed.

Controlling authority is the current user instruction, including its approved no-shutdown-residue-publication decision and explicit leftover-reference scope. Applicable repository authority includes AGENTS.md §§11–12 and 16, agent-rules, reference authority hierarchy, spec-driven-development and maintainer-style, and Bifrost staging/recovery/shutdown authority. The current Bifrost shutdown description at `architecture/bifrost-design.md:323–327` assigns retained staging to restart. Lower-priority descriptions cannot restore the deleted sweep. FIND-007-3, excluded Postgres columns, the existing public error, and no live-read cap stand.

Static only: no cargo, nextest, mise, builds, tests, benchmark, commits or reviewed-source edits. No CodeGraph index exists. Only this assigned validation report is written. Execution results are supplied evidence, not fresh verification.

## Independent producer, owner and consumer trace

| Boundary inspected | Source evidence and disposition |
|---|---|
| Removed publication producer | The full `ScribeImpl::shutdown`, `scribe/mod.rs:1315–1412`, still closes admission, flushes/drains shards, closes/drains persistence, then closes execution lanes and finalizes owners. The correction deletes only its `publish_staged_residue` sweep and the helper. The finalizer at :1443–1463 aborts retained handles and closes WAL streams; it does not delete stage files. |
| Durable stage precedes replay retirement | `PersistenceWorker::persist_once`, `persistence.rs:1890–1908`, stages the member before manifest advance; `stage_member` registers the durable stage. It then invokes ordinary target/dwell publication. That accepted-work publication may finish during drain; it is distinct from sweeping every below-target member. Removing the sweep does not remove the durable staging boundary. |
| Restart and idle publication | `replay_wal_async`, `mod.rs:2480–2518`, restores stage, reconciles publications and resumes original claims before WAL replay/readiness. `staging_runtime.rs:477–537` validates and reconstructs nonterminal ready/claimed authority; `hot_stage.rs:457–461` preserves `ready_at`; `assembly.rs:800–857` restores ready members or validates original claim identity. `due_key_for`, :1011–1033, selects target/dwell; the production server scanner, `app/server.rs:551–579`, invokes `publish_due` without another write. |
| Surviving explicit flush producer | Full `ScribeImpl::flush_staged`, `mod.rs:2450–2469`, flushes/drains admitted generations and explicitly calls `PersistenceRuntime::publish_residue(ClaimCause::Drain)`, then retires committed copies. This remains a real production caller, including the server flush facade in `state.rs:2473`. |
| Residue enumeration consumers | Full `PersistenceRuntime::publish_residue`, `persistence.rs:937–948`, delegates to the retained worker. Full worker method, :2036–2058, resumes retryable original claims and sweeps `staging.ready_keys()`. `ScribeStagingRuntime::ready_keys`, `staging_runtime.rs:678–684`, delegates to full `StagingAssembler::ready_keys`, `assembly.rs:987–993`. The sibling enumeration at `persistence.rs:2088` is partition-scoped test-support; unit tests also use the index. No shutdown call remains. |
| Journey contract consumer | Full `scribe_shutdown_drains_or_preserves_replay`, `wyrd-testing/tests/bifrost/scribe/lifecycle.rs:127` onward, stops/restarts retained roots, permits zero or complete publication, reads acknowledged rows exactly once, and separately covers abrupt replacement with writable and staged evidence. The introductory prose and stop comment promise more than those assertions and current shutdown provide. Assertions themselves are not the defect. |

Search of current runtime source finds no `publish_staged_residue` reference. Historical task/review records are historical and are not candidates for rewriting. The unchanged failed-claim tick-selection issue discussed by the data reviewer does not follow from deleting shutdown's ready-key sweep; this confirmation does not expand into an adjacent retry redesign. Startup's original-claim resumption remains present. No claim of data loss, duplicate replay, failed restart publication, or broken executable test is retained.

## Adjudication of every proposed claim

| Discovery ID | Validation | Independent resolution |
|---|---|---|
| BEH-R5-1 | REVISED | Confirm the explicit false shutdown guarantees in assembly docs and lifecycle prose. Narrow the proposed correction: a queue-drain reference alone does not promise a shutdown sweep; preserve explicit flush and all executable assertions. Consolidated as FIND-007-13. |
| INV-R5-001 | REVISED | Confirm the active reliability guide's “closes residue claims,” assembly's “at shutdown every remaining key,” and lifecycle's mandatory-publication guarantee. Ordinary admitted publication and the all-or-none assertion remain legitimate. Consolidated as FIND-007-13. |
| RSTD-R5-1 | REVISED | Confirm the definite false guarantees, under explicitly requested leftover references. Reject `mod.rs:1471`'s “or for shutdown” as an independent blocking defect: its positive tick behavior remains correct. Reject `persistence.rs:575` as standalone evidence: explicit flush still drains the queue before using the retained worker. Consolidated as FIND-007-13. |
| FUP-R5-1 | REVISED | Its principal distinction between obsolete sweep promises and valid flush/admitted-work references is source-proven. Retain only definite false guarantees and their directly associated journey account. No runtime correction, new check, or shutdown-publication restoration is justified. Consolidated as FIND-007-13. |

The conflict between discovery reports is resolved from scope and authority. A stale comment is not an executable durability regression, as the system/domain reviewers correctly establish. The user nevertheless expressly requests leftover-reference checking, and these current references directly promise the deliberately removed lifecycle behavior. Their false durable-boundary account is therefore in scope. This is a single documentation-correctness issue; no runtime failure is inferred from it.

## Final deduplicated ledger

### FIND-007-13 — REVISED — REGRESSION (documentation)

**Discovery sources:** BEH-R5-1, INV-R5-001, RSTD-R5-1, FUP-R5-1.

**Violated obligation:** current user item 2 includes leftover references; current Bifrost shutdown authority preserves staged residue for restart. AGENTS.md §16 makes workflow/invariant documentation part of implementation correctness. These current descriptions still guarantee the removed sweep.

**Exact locations and evidence:**

- `architecture/references/domain/analytical-operations-reliability.md:40–43`: shutdown “closes residue claims.” This still assigns forced residue settlement to shutdown.
- `crates/vala/vala-bifrost-redux/src/scribe/assembly.rs:437`: `ClaimCause::Drain` claims graceful settlement before shutdown, although its surviving production producer is explicit flush.
- `crates/vala/vala-bifrost-redux/src/scribe/assembly.rs:982–985`: `ready_keys` says “at shutdown every remaining key is claimed as residue” instead of being left for dwell. That caller was deleted; retained ready members now have restart/tick publication.
- `crates/wyrd/wyrd-testing/tests/bifrost/scribe/lifecycle.rs:106–110`: the journey rustdoc promises a graceful stop sweeps every held stage member and leaves nothing for restart to rediscover. The comment at :149 promises shutdown owes staged rows publication. Its `# Panics` account at :122–124 and related narrative at :159–162 must describe the retained replay/publication proof rather than mandatory publication by stop.

**Observable consequence:** maintainers following current operational guidance or the journey's stated contract are told clean stop has settled all staging and restart has no retained stage to restore. The candidate intentionally guarantees neither. This misstates the accepted durability boundary and the proof furnished by the retained test. No loss, duplication, runtime test failure or availability consequence is asserted.

**Diagnosis:** deletion at the shutdown owner removed `shutdown -> publish_staged_residue -> publish_residue(Drain)` while leaving active prose for that former producer. This is one common source, rather than separate durability, test, and standards failures. Existing runtime/recovery behavior already implements the approved outcome; no consumer guard belongs here.

**Decision-complete smallest correction:** delete the false shutdown guarantees or replace them in their existing locations with the accepted staged-retention/restart account. Describe the existing `Drain` cause as explicit residue flush. Align the journey's prose with exactly-once readback after restart, its permitted zero-or-complete publication result, and its existing later publication proof. Reuse the already accurate shutdown comment at `mod.rs:1369–1371` and explicit-flush worker docs at `persistence.rs:2018–2022`; add no new lifecycle mechanism. Correct the active reliability reference without deleting its valid admitted-work drain statement.

**Preserve:** all runtime code, `ClaimCause::Drain` and metric labels, explicit `flush_staged`, residue methods, normal target/dwell publication, accepted-work drain, source leases, ACK/WAL/manifest/claim fences, all executable assertions and historical evidence. Do not rename a test, weaken its all-or-none assertion, require publication at shutdown, or broaden into unrelated prose cleanup. Clarification of the worker-field wording is unnecessary to close the retained finding because its actual explicit-flush use remains true.

**Ponytail ladder:** the invalid prose can be deleted while preserving the whole task. Existing owner documentation supplies the correct wording. No dependency, helper, abstraction, test harness, new check or runtime test is needed. A prose-only correction is sufficient and testable statically.

**Focused closure proof:** inspect the corrected current guide, assembly docs and lifecycle prose against full shutdown/flush bodies; search their active shutdown/residue descriptions to ensure none promises a forced sweep; confirm `flush_staged -> publish_residue(Drain)` and restart/tick paths remain untouched; verify the correction changes documentation only and preserves all assertions. Existing supplied runtime evidence need not be rerun to prove this wording correction in this static-only review.

## Independent prior-finding closure

| Finding | Source / evidence validation | Disposition |
|---|---|---|
| FIND-007-10 | `app/server.rs:681–690` chooses stopping IO iff the retained Bifrost owner has Scribe; `grpc/mod.rs:405–478` mounts the same actual ingest capability. Oracle-only listener calls the original unchanged `serve_grpc_with_listener`; public listener remains original. Full StoppingIo body retains token-driven accepted IO interruption independent of body demand, socket forwarding and exact `TcpConnectInfo`. Activation, deadlines and later role cleanup are unchanged. Supplied blocked-window/lost-Scribe/peer results corroborate; no new Oracle-only timing execution is claimed. | CLOSED |
| FIND-007-11 | Full AsyncRead/AsyncWrite/Connected implementations at `wyrd-tonic/src/server/mod.rs:321–412` now document errors for read, write, vectored write, flush and shutdown, and document ConnectInfo. Cancellation refusal is correctly absent from delegated write-half shutdown. | CLOSED |
| FIND-007-12 | R5 appended evidence records classifier lib exact selector (1 pass) and three oracle journey exact selectors inside the repository Postgres wrapper (3 pass). Independently located classifier at `dispatcher.rs:2527`, footer journey at `distributed.rs:1877`, and lost-Scribe/window journeys at `peer_network/analytical.rs:1136,1253`. Package/target/module names match. RED attribution is retained without inventing reviewer runs. | CLOSED |

No independently retained runtime finding remains. Supplied redux integration 872/872, Scribe journeys 20/20, server journeys 26/26, peer 11/11 and focused 1+3 runs remain attributable evidence. The 100M-row benchmark supports shutdown cost, not independently restart recovery. Source traces and existing recovery/idle-publication seams support the latter; this report claims no fresh combined restart/tick execution.

**Validated result: bounded correction required for FIND-007-13 only; prior FIND-007-10/11/12 closed.**
