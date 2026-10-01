# Concurrency and resource ownership review

Overall result: **PASS**. Proposed material findings: **none**.

## Subject and authority

Candidate `1a66d8a7b4583c9798c2f1573dc6ab1f3eadb027`, parent `ca99db0af5a0d898ef67834699405c1c73719f56`, cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`, TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. HEAD was rechecked and remains the candidate. Reviewed the cumulative relevant source/diff, original task obligations, revision 20 spec, prior R5 diagnosis and current evidence. This report covers concurrency, cancellation, accepted connection lifetime, staged-reader ownership and the shutdown-to-restart handoff; it does not reopen the maintainer's shutdown publication decision.

Governing obligations: spec REQ-003/004/005/014/015 and INV-004/005/007; R5 Oracle-only preservation; AGENTS ownership, asynchronous-boundary and testing rules; agent-rules; spec-driven-development and maintainer-style; Bifrost design live-tail, assembly, recovery and shutdown boundaries. Fixed FIND-007-3 accounting disposition, no cap, error naming and Postgres exclusion stand.

## Reviewed boundary and evidence

| Boundary | Source evidence | Result |
|---|---|---|
| Role-sensitive transport lifetime | `app/server.rs:675–695` chooses stopping IO iff the retained Bifrost owner has Scribe. `grpc/mod.rs:405–478` mounts fragments through that same Scribe capability. Query-only pods use the unchanged `serve_grpc_with_listener`; mixed pods retain the intentional Scribe stop boundary. Public gRPC still uses the graceful runner. | PASS |
| Cancellation while HTTP/2 stops polling a body | `wyrd-tonic/src/server/mod.rs:267–407` retains a pinned owned token future per accepted socket. Read/write/vectored-write/flush poll it and register the connection-driver waker; cancellation refuses socket IO independently of response-body demand. Shutdown of the write half remains delegated. | PASS |
| Connection identity and resource drop | `StoppingIo::Connected` forwards `TcpConnectInfo`. `OraclePeerGrpc::execute_fragment` (`peer_service.rs:546–605`) retains the native fragment stream inside its response producer; teardown therefore drops the follower batches, source references and staged holds. Body polling also selects server cancellation. Neither path adds a fragment expiry or detached producer. | PASS |
| Live-source producer ownership | `tail_rpc.rs:572–699` snapshots shallow memtable references and leases staged sources, excluding generations already represented by memory. `follower.rs:779–868` moves staged protection into `HotParquetExec` and projected references into the memory leaf; construction errors drop the untransferred owners. | PASS |
| Scan lifetime and cancellation | `exec.rs:2668–2673,2735–2742,2880–3010` keeps staged protection in both the leaf and each stream. The stream's drop guard cancels outstanding metadata acquisition. `hot_source.rs:239–264` releases every registered staged hold through Drop. Memory uses the existing follower session pool. | PASS |
| Leader cancel/deadline/intentional stop | `live.rs:496–637` keeps the child stream inside the plan producer; cancelling/deadline paths return through that owner, and dropping a completed-plan child drops its stream. Natural EOF still requires completion; failure after delivered rows stays fatal. | PASS |
| Publication versus an open read | Publication changes source authority, while existing leases continue to protect the old runs. `tail_rpc.rs` excludes overlapping staged generations; lease Drop owns release. No R5 change bypasses these protections. | PASS |
| Shutdown handoff | `scribe/mod.rs:1315–1508` closes admission, flushes/drains shards, closes/drains the accepted persistence queue, then closes execution lanes and finalizes retained workers. Deleting the residue sweep does not delete staged members or release their durable authority. Cancellation/deadline retains the existing abort finalizer. | PASS |
| Restart ownership and later publication | `scribe/mod.rs:2480–2534` restores staging, reconciles published operations and resumes durable claims before WAL replay/readiness. `staging_runtime.rs:477–537` reconstructs ready versus outstanding ownership; `assembly.rs:800–855` rejects duplicate/mismatched claim membership. `app/server.rs:551–579` drives `publish_due` every second without new writes; `assembly.rs:1011–1033` selects target/dwell-due ready members. | PASS |

## FIND-007-10 closure

The invalid connection semantics were produced at private-listener runner selection. R5 corrects exactly that producer, without changing Oracle admission, Analytical cancellation or durable settlement. `Bifrost::begin_shutdown` (`state.rs:1927–1938`) removes readiness first; supervision (`app/supervise.rs:180–202`) then cancels transport. For an Oracle-only listener the transport future now follows the same graceful runner as before R4. The later role drain remains responsible for engine cancellation. For a Scribe-containing listener, token cancellation still interrupts parked socket IO and releases fragment sources before role drain. The correction preserves the process deadline and does not grant unlimited drain.

No new Oracle-only journey was added. The source restoration is exact at the owning runner boundary; supplied peer-suite evidence supports neighboring behavior. This static review does not claim to have executed or observed a fresh Oracle-only timing scenario.

## Shutdown decision and leftovers

The removed `publish_staged_residue` has no remaining executable reference. The remaining `publish_residue` call is the explicit `flush_staged` operation (`scribe/mod.rs:2458`); it remains necessary for explicit flush and does not make shutdown publish residue. A partition-scoped residue path is test-support behavior. Persistent staging survives finalization and is reconstructed before restart admission; the periodic publication owner remains installed after restart.

Two unchanged explanatory comments retain old shutdown wording: `assembly.rs:437` on `ClaimCause::Drain`, and `assembly.rs:981–986` on `ready_keys`. They describe the former residue sweep but do not invoke it or alter ownership. No reachable concurrency, replay or durability regression follows from these comments; they are not proposed as a correctness finding under the requested scope.

## Verification limits

Static review only: no cargo, nextest, mise, builds, tests or commits were run. Supplied evidence includes the classifier and three focused remote-footer/lost-Scribe/blocked-window recipes, server peer 11/11, fmt/lints, redux integration 872/872, Scribe journeys 20/20, server journeys 26/26 and the 100M-row clean shutdown benchmark. Existing source checks include staged protection across publication, dropped-scan release and terminal-safe live frames. Those results are supplied historical evidence, not independently rerun verification.

No material concurrency/resource-ownership finding remains. Result: **PASS**.
