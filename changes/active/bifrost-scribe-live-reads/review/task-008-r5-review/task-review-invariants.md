# Independent invariant review — TASK-008-R5

**Result: FAIL. FIND-007-10, FIND-007-11 and FIND-007-12 are closed; one bounded leftover-reference finding remains. No runtime durability, replay or restart-publication defect found.**

## Subject and limits

Candidate `1a66d8a7b4583c9798c2f1573dc6ab1f3eadb027`, parent `ca99db0af5a0d898ef67834699405c1c73719f56`. Cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`; TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. Read the immutable subject, original TASK-007/TASK-008, approved spec revision 20, R4 verdict and R5 remediation/evidence, cumulative changed-surface map and relevant cumulative/correction source. Authority: current confirmation scope and fixed maintainer decisions, AGENTS.md, agent-rules, spec-driven-development, maintainer-style, Wyrd design/doctrine, Bifrost design and applicable reference router/domain guidance.

Static only: no cargo, nextest, mise, build, test, benchmark or commit performed. Supplied results are attributed evidence, not fresh verification. HEAD rechecked unchanged. Only this assigned report was written. No `.codegraph` index exists; navigation used existing repository source/search.

The shutdown decision is accepted: shutdown must preserve staging rather than sweep residue into object publication. FIND-007-3, excluded Postgres columns, existing error code and absence of a live-read cap remain excluded decisions.

## Acceptance matrix

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-014/AC-016: staged files use shared pruning-capable Parquet scan, memory source uses engine partitions/predicate | `oracle/follower.rs:784–868` constructs `MemorySourceConfig` and `HotParquetExec`, session partitions, signed residual filter; staged decoder remains removed | Existing focused staged pruning/projection/partition and lease tests recorded in TASK-007/TASK-008 | PASS |
| REQ-014: native local Arrow path preserves output counters/terminal and remote wire checks | `dispatcher.rs` LiveFrame/NativeOutputTally; `oracle/live.rs` LiveFrameDecoder; `oracle/peer_service.rs` RPC encoder remains wire boundary | Native completion/footer tests and peer service focused evidence in packet | PASS |
| REQ-015/AC-017: no row tenant field/filter/tripwire, footer authenticated at every producer and checked before rows without fallback | `parquet/footer.rs:26–54`; `scribe/parquet_writer.rs:603`; `claim_assembly.rs:266`; Forge rewrite properties use tenant; `oracle/exec.rs:1040,1115–1151,2961` verifies published/cached hot/staged metadata | Focused footer negatives, cached/footer/COUNT(*) journeys and codegen/docs evidence in packet | PASS |
| In-memory isolation and pod-local staging invariant | Follower resolves authenticated binding/schema before `tail.open`; metadata keys use assignment tenant; staged scan is constructed on Scribe follower and holds lease | Follower, staged hold and live-read journeys supplied | PASS |
| Prior FIND-007-9: remote footer refusal cannot degrade into availability | `dispatcher.rs:1813–1818` retains Aborted as TenantInvariant; shared classifier and live reader distinguish security from omission | R5 exact classifier 1 pass, remote footer journey within 3-pass exact selection | PASS / preserved closure |
| FIND-007-10: stopping IO applies only to Scribe listeners; Oracle-only keeps original graceful interval | `app/server.rs:682–690` branches on composed `bifrost.scribe().is_some()`; `grpc/mod.rs:405–478` composes corresponding role services; public listener unchanged | Supplied blocked-window/lost-Scribe focused journeys and server peer 11/11; Oracle-only restoration statically uses exact original runner | PASS / CLOSED |
| FIND-007-11: documented stopping IO errors and connect identity | `wyrd-tonic/src/server/mod.rs:322–412` Errors sections for read/write/vectored-write/flush/shutdown and ConnectInfo docs match actual delegation and cancellation | Supplied lints exit zero; static item inspection | PASS / CLOSED |
| FIND-007-12: reproducible focused evidence for all named closure checks | R5 task Implementation evidence records exact source names, targets, wrapper and pass counts; selectors match source | Supplied classifier 1 pass and combined footer/lost-Scribe/window-blocked selection 3 pass | PASS / CLOSED |
| Approved shutdown: stage and preserve residue without shutdown sweep | `ScribeImpl::shutdown:1342–1382` flushes/drains staging before closing lanes; deleted `publish_staged_residue` has no runtime reference | Supplied redux integration 872/872, Scribe journeys 20/20, server journeys 26/26 and benchmark clean stop | PASS |
| Durability and replay: acknowledged rows survive unpublished staging and resume exactly once | `persistence.rs:1896–1906` durable stage precedes manifest advancement; `staging_runtime.rs:477–537` restores exact ready/claimed/terminal authority; `mod.rs:2489–2508` restores/reconciles/resumes before WAL replay and readiness | Supplied Scribe restart/replay journeys; existing mixed terminal-claim recovery test inspected | PASS |
| Restart publication without new writes | persisted ReadyMember ready_at restores into ordered ready index; `assembly.rs:677–685,1013–1029` dwell uses original timestamp; server scanner `app/server.rs:555–576` invokes publish_due each tick | Supplied redux integration/Scribe/server journey evidence; static reachability | PASS |
| Leftover references agree with approved shutdown behavior | Active derived reliability guidance, assembly rustdoc and journey comment still describe shutdown residue publication | Static exact search/source inspection | FAIL: INV-R5-001 |
| Non-goals/fixed decisions/source immutability | No new caps, compatibility path, excluded DB changes or reopening maintainer decisions proposed | Static scope; concluding HEAD matches subject | PASS |

## State producer-to-sink trace

The durable boundary is staging, not object publication. `PersistenceWorker::persist_once` encodes and registers a staged generation before advancing the stream manifest, then independently publishes claims made due by target/dwell. A shutdown whose final generation is below target can therefore retire its WAL only after durable staged records exist. Removing the residue sweep changes no ACK, staging fsync, manifest ordering or file-list transaction fence.

`ScribeImpl::shutdown` closes external admission, flushes shard generations, drains shard and persistence queues, then closes lanes and releases owners. Finalization clears tasks and WAL stream handles; it does not delete staged members. If any bounded phase cannot finish, existing WAL/staging recovery evidence remains authoritative. Accepted-work publication already occurring through normal persistence may finish during drain; no extra residue sweep is started.

Startup restores staged authority before replay: unclaimed ready members return to the assembler; existing claim ownership retains original identities; terminal claim reconciliation retires surviving members without republishing. Ready records retain ready_at, so dwell is not reset by restart. Boot refuses readiness if any recovery step fails, and the server's supervised lifecycle tick drives target/dwell publication without another write. The same lease protection used for live staged scans continues to defer member deletion while a reader owns the files.

The listener branch uses the same composed Scribe capability as the fragment adapter. Oracle-only accepted IO uses the unchanged graceful helper; mixed/Scribe listeners retain StoppingIo and exact TCP/TLS connect info. The cancellation token wakes the socket driver even when response body polling is blocked by HTTP/2 capacity. Subsequent role cancellation, deadlines and lease settlement owners are unchanged.

## Proposed finding

### INV-R5-001 — REGRESSION: active shutdown references still require removed residue publication

**Violated obligation:** the explicit confirmation scope requires checking the approved no-shutdown-publication decision for leftover references; active derived guidance and workflow documentation must describe the actual durable boundary under `architecture/bifrost-design.md:323–327`, which now preserves staged residue for restart.

**Exact locations:**

- `architecture/references/domain/analytical-operations-reliability.md:40–43`: "Shutdown ... closes residue claims, and drains admitted publication" still instructs the removed shutdown sweep.
- `crates/vala/vala-bifrost-redux/src/scribe/assembly.rs:982–985`: ready_keys rustdoc says "at shutdown every remaining key is claimed as residue".
- `crates/wyrd/wyrd-testing/tests/bifrost/scribe/lifecycle.rs:148`: "A pod told to stop owes its staged rows a publication." Nearby comments at 159–162 also attribute object publication to drain lasting long enough instead of normal in-flight publication/restart.

**Evidence and consequence:** the new producer is `ScribeImpl::shutdown:1369–1371`, explicitly preserving below-target members; its sweep helper is deleted. `ready_keys` remains reached by explicit flush/test partition residue publication, not shutdown. These reachable active guidance/test descriptions instruct a maintainer to restore the behavior deliberately removed and describe publication as a shutdown obligation; the implementation and principal architecture now disagree with those descriptions. This is a documentation regression, not acknowledged-row loss or an argument for reverting the decision.

**Smallest testable correction:** update these existing descriptions to distinguish explicit residue flush from shutdown. Shutdown closes/drains accepted staging and preserves unpublished members; restart restores them and the normal lifecycle tick publishes due claims. Preserve code, tests and publication fences. Existing assertion permitting zero or the complete claim after restart remains valid; change misleading text, not its accepted outcomes. Historical task/review records describe historical decisions and should remain intact. Focused closure is static search and caller/source comparison showing no active reference assigns a shutdown residue sweep; no runtime execution or new harness is needed.

No further material findings proposed. New private abstractions/configuration are not needed for the listener correction; reuse of the existing two runners is the minimum owner-boundary fix.
