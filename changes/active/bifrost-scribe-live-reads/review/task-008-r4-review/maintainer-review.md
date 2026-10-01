# Independent maintainer review — TASK-008-R4

Result: **PASS**. No material maintainer findings.

## Subject and limits

Candidate `ca99db0af5a0d898ef67834699405c1c73719f56`, correction parent
`2f188cb6185061a43db36122aad68b5e253308d1`, cumulative TASK-007 base
`a7582db587c6170a290760f1741673125612b797`, TASK-008 base
`f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. HEAD was checked and remained
the candidate. This is an independent static maintainer pass; no build, test,
mise command, commit, source modification, or current sibling report was used.

The review covers the original task owners and their cumulative relevant
changes, not merely the final patch. TASK-006 intersections are considered at
the live-read transport, test-harness, configuration and shutdown boundaries;
unrelated benchmark completion is excluded as directed. Maintainer decisions
on FIND-007-3, Postgres tenant columns, the public error code and cap removal
stand.

Authorities: `AGENTS.md`, `architecture/agent-rules.md`, reference routing,
`languages/maintainer-style.md`, `languages/spec-driven-development.md`, Rust
ownership guidance, the applicable Wyrd design/doctrine service boundaries,
and Bifrost table identity, live-tail, memory, cancellation and audit authority.
The original TASK-007/TASK-008 obligations and R4 remediation constrain the
review threshold. Implementation evidence is verification evidence, not an
authority for accepting code shape.

## Changed-surface coverage

| Surface | Owner, callers and tests inspected | Maintainer assessment |
|---|---|---|
| Per-file identity and removal of row tenancy | `parquet/footer.rs`: `BifrostFooterIdentity`, tenant stamping/proof and duplicate-key rejection; writer-properties change; `ClaimAssembler` request/encoder plumbing; Scribe envelope/writer changes; Forge rewrite policy/executor calls; managed envelope and schema declarations | Tenant identity is explicit, typed and carried by the existing owners. Deterministic footer helpers remain local to their responsibility. Removal does not leave a parallel row filter or codec mechanism as another authority. Tests assert actual footer identity and schema outcomes. |
| Shared staged/published scans | `oracle/exec.rs`: `PublishedFooterLoader::load`, shared tenant proof, hot reader metadata, `HotParquetExec` and `hot_stream`; catalog provider error conversion; leader `Oracle::audit_tenant_refusal` | Proof occurs at the reader boundary, preserves the typed cause and precedes decode. Lease/cache/cancellation lifetimes are documented beside their owners. Published and staged paths reuse existing scan machinery rather than duplicating a decoder. |
| Scribe source and cap-free cut | `ScribeTailResolver::live_leaf`, `LiveTailBatches::into_parts`, memtable readable collector/provider cut, shard snapshot entry points; `ScribeProviderCut`, v6 authority digest, private conversion and reserved protobuf fields | Selection, writer identity and resource ownership remain discoverable. Memtable projection and staged scan construction have one resolver owner. Cap removal is consistently reflected in request documentation and wire/digest shape. The fixed normative digest test and conversion tests express the changed contract. |
| Native and remote fragment path | `ScribeFragmentExecutor`, `OraclePeerGrpc::execute_fragment`, `dispatch_status`, tonic `execute_candidate` and all status classifiers; dispatcher classification test, live/follower tests and distributed refusal journey | Native batches and wire frames remain distinct closed variants. The R4 change belongs at the shared streamed-status conversion that loses the cause. Its documentation explains why initial-open classification alone cannot protect lazy footer refusal. |
| Peer socket shutdown and public listener | Complete new `StoppingIo` type, constructor, token poll and IO/Connected implementations; `serve_peer_grpc_with_listener`; both listener task branches in `BoundServer`; peer router TLS construction; `TransportPlane::admits`; retained `execute_fragment` cancellation | The wrapper owns meaningful socket/token state. It has no new service abstraction, configuration or dependency. IO forwarding and `TcpConnectInfo` preservation are explicit. The only production caller selects it for the private listener; the public listener retains its existing serving function. Connection lifetime enforcement belongs in tonic transport rather than another body-level guard. |
| Absolute local paths | `BifrostDataRoot::prepare`, derived volume roots and resolver staged IO; relative-root test | Resolve once at the root owner with `std::path::absolute`; downstream writer and reader do not need separate correction paths. Error and path semantics are documented. |
| Regression journeys and evidence helpers | Entire `remote_staged_footer_refusal_fails_closed`, `staged_runs`, `rewrite_footer_tenant`; entire `prove_window_blocked_scribe_stop`; adjacent paused-producer release journey; `PeerCluster::stop`; `observe_live_production_for_test`, open-producer counter and new batch counter | Tests state the exact caller-visible failure, isolate the remote producer and inspect terminal/release outcomes. Test-support counters reuse the existing observation boundary and are excluded from production. Helpers sit beside the journeys they serve; graceful stop reuses cluster lifecycle. The large ordinary output is necessary to exercise the diagnosed transport window rather than synthetic host load. |
| Documentation and review evidence | Architecture live-tail correction, tenant envelope/footer documentation changes, R4 task evidence and TASK-006 whitespace correction | The live-tail paragraph now describes shallow cap-free snapshots and query memory governance without deleting writer-epoch, signed-predicate, deadline or source-lifetime requirements. R4 evidence records focused recipes and RED/GREEN failures; permanent code does not carry finding IDs. |

## R4 closure and regression assessment

* **FIND-007-7:** the final correction removes whitespace-only lines in existing
  evidence; there is no behavior or documentation-content change. Supplied
  verification records both required whitespace bases passing.
* **FIND-007-8:** `architecture/bifrost-design.md:312` no longer promises retained
  byte/batch limits. The description agrees with the cap-free cut and collector.
* **FIND-007-9:** `oracle/dispatcher.rs:1813` preserves `Aborted` as
  `TenantInvariant`. The adjacent test retains availability and permission
  behavior. The new remote staged journey uses a staged-only source and proves
  zero rows, failed terminal, the existing tenant reason, and one leader event.
* **FIND-007-10:** `wyrd-tonic/src/server/mod.rs:265` implements connection-owned
  cancellation, with `StoppingIo` forwarding the TCP connect-info type required
  by the existing TLS extension consumer. `app/server.rs:678` chooses this only
  for the private peer task. The window-blocked journey stops the Scribe before
  resuming client reads, requires clean cluster stop, checks producer release,
  then checks read failure and Oracle admission release. This is materially
  different proof from a body that remains pollable while paused at its source.

No concrete maintenance defect was found in naming, module placement, typed
arguments, failure propagation, test clarity, documentation or owner shape.
No optional refactor or equally clear alternative is proposed. There are no
Python/TypeScript binding declaration changes in these task surfaces requiring
an independently authored declaration to mirror the new private IO wrapper.

## Verification assessment

Runtime checks were deliberately not rerun. Supplied R4 results are fmt,
lints, docs check, Oracle journeys 42/42, peer tests 11/11, focused RED/GREEN
recipes and both whitespace checks. Source inspection establishes that the
new proof reaches the corrected boundaries; the reported runs supply runtime
evidence. This report does not independently reproduce timing or build claims.

Proposed finding ledger: **empty**. Overall maintainer result: **PASS**.
