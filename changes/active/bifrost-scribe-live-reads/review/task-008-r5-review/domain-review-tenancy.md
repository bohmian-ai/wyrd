# Independent tenancy and security review — TASK-008-R5

**Result: PASS. Material proposed findings: none.**

## Subject and limits

Candidate `1a66d8a7b4583c9798c2f1573dc6ab1f3eadb027`; correction parent `ca99db0af5a0d898ef67834699405c1c73719f56`; TASK-007 cumulative base `a7582db587c6170a290760f1741673125612b797`; TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. HEAD was rechecked and remained the candidate.

Reviewed TASK-008, specification revision 20, R4 tenancy report as prior-closure hypotheses, R5 remediation, latest correction, cumulative changed trust-boundary source and its producers/consumers. No current sibling discovery conclusions were used. Static only: no cargo, nextest, mise, tests, builds, benchmarks, commits or implementation changes. Supplied green verification is implementation evidence, not independently executed proof.

The approved decisions remain fixed: FIND-007-3, Postgres tenant columns, `WYRD_VALA_500_QUERY_TENANT_INVARIANT`, absence of a live-read cap, and shutdown leaving durable residue staged.

## Boundary and authority coverage

| Boundary | Authority | Inspected source |
|---|---|---|
| Authenticated tenant producer and immutable binding | AGENTS.md §§2, 3, 9; agent-rules; wyrd-design/doctrine; spec REQ-015 and INV-002/006; bifrost-design tenant identity | Scribe seal-bound encoder, staging runtime restored contexts/claim assembly, Forge managed executor/policy, footer identity and writer properties |
| Before-row file refusal on every tier | TASK-008 R2–R6; bifrost-design; datafusion and olap-serving references | PublishedFooterLoader, shared footer proof, HotParquetExec/hot_stream, authenticated follower staged source construction, cache keys |
| Remote classification and audit ownership | Canonical audit rules; security posture; spec REQ-004/015 | Server peer claims/digests/preflight; scribe_stream_error/dispatch_status; dispatcher execute_candidate/stream_status_error; live failure mapping; Oracle leader audit wrapper and server query_audit |
| Peer transport identity and separation | wyrd-security-posture peer identity; R5 preservation constraints | BoundServer listener selection; grpc peer router/TransportPlane; wyrd-tonic TLS and StoppingIo; pinned tonic 0.14.6 Connected/TLS implementation and enabled feature |
| Restart does not widen tenant authority | Approved shutdown decision; bifrost-design recovery | Scribe shutdown/replay_wal_async; staging_runtime restore_context/assembly; explicit flush_staged residue caller |

## Source assessment

**Authenticated identity remains consistent from producer through reader.** `ParquetBatchEncoder::prepare_sorted_candidate` (`scribe/parquet_writer.rs:381–405`) compares binding, seal tenant and frozen seal key before encoding. ArtifactPlan carries that tenant into BifrostFooterIdentity. Staging claim assembly uses `context.binding.tenant` (`staging_runtime.rs:708–724`). Recovery resolves the binding from the durable key and requires the registered recipe to reproduce that key (`:620–665`). Both Forge rewrite paths pass `self.binding.tenant` to writer configuration (`forge/managed/executor.rs:198–203,285–290`), which stamps `wyrd.bifrost.tenant`. None of the R5 edits replace these bindings or restore a row tenant fallback.

`verify_footer_tenant` (`parquet/footer.rs:49–55`) requires one matching value; `footer_value` also rejects missing, valueless and duplicate values. PublishedFooterLoader checks both cache hits and misses before returning reader metadata (`oracle/exec.rs:985–1046`). Hot/staged scans call the same proof through `tenant_proven_reader_metadata`, before Arrow reader construction and row-group decode (`:1115–1151,2960–2963`). Follower staged cache identity comes from the authenticated assignment (`follower.rs:814–823`), whose tenant was already checked against verified claims and digest/preflight by the receiving server (`peer_service.rs:283–350`). Memory sources continue to resolve through the seal-qualified tail snapshot rather than arbitrary caller batches.

**Remote tenant refusal remains terminal and attributable.** `scribe_stream_error` preserves QueryTenantInvariant; `dispatch_status` encodes it as Aborted (`peer_service.rs:647–658,690–698`). The already-open response stream uses `stream_status_error`, which retains Aborted as TenantInvariant (`dispatcher.rs:1501–1505,1813–1823`). `live.rs:618–647` excludes this class from availability loss and restores the typed public error. Therefore a missing/foreign footer cannot become a successful degraded omission merely because a schema frame preceded the refusal. `audit_tenant_refusal` wraps leader execution and uses a taken Option to emit at most once (`oracle/mod.rs:2459,2484–2520`). Audit attribution uses the verified query context, never footer bytes; the server stages through TenantConn and canonical `audit::append_on`, with the existing tracked non-blocking failure semantics (`oracle/query_audit.rs:81–107,148–167`). Prior FIND-007-9 remains closed.

**R5 preserves peer mTLS.** Both runner branches receive the same already-built mutual-TLS peer router. The Scribe branch alone wraps accepted sockets; Oracle-only peers choose the unchanged graceful runner (`app/server.rs:678–691`), while public gRPC remains separately served (`:658–664`). StoppingIo forwards TcpConnectInfo unchanged (`wyrd-tonic/server/mod.rs:402–411`), and pinned tonic's TLS Connected implementation wraps that type with the authenticated certificate chain. The enabled `tonic/tls-connect-info` feature preserves the exact `TlsConnectInfo<TcpConnectInfo>` extension required by `TransportPlane::admits` (`grpc/mod.rs:164–180`). Mandatory client CA verification and the fixed DNS identity check remain fail-closed. The wrapper supplies no tenant, principal, fence or assignment authority. Security preservation relevant to FIND-007-10 passes; complete lifecycle qualification belongs to the concurrency/system reviews.

**The shutdown decision does not alter tenant recovery.** Shutdown still flushes admitted generations and drains persistence before closing lanes (`scribe/mod.rs:1350–1373`), retaining staged files rather than rewriting their identity. Startup restores staging, reconciles durable publications and resumes claims before WAL replay/readiness (`:2483–2515`). The remaining `publish_residue` call in `flush_staged` (`:2457–2459`) is an explicit flush operation, not a leftover shutdown call. No new recovery identity or foreign-tenant compatibility path is introduced.

## Verification assessment and result

Supplied focused remote-footer refusal, lost-Scribe and blocked-window recipes, classifier proof, server-peer 11/11 and broader redux/Scribe/server journey results support the affected runtime boundaries. Existing focused footer tests explicitly cover missing, foreign and duplicate values; test source and shared error paths were inspected. This review establishes static boundary correctness and does not independently certify runtime timing or benchmark results.

**PASS. No material tenancy/security findings.** Remote refusal closure is preserved; R5 runner selection and documentation introduce no authenticated-identity, audit-attribution or peer-authentication regression. The approved shutdown change retains the same tenant-bound restart mechanism.
