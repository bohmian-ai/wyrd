# Independent invariant review

**Result: FAIL — one proposed scope finding; FIND-007-7/8/9 closed, FIND-007-10 corrected for Scribe connections but broader Oracle-only shutdown behavior needs correction or validation.**

## Subject and limits

Immutable candidate `ca99db0af5a0d898ef67834699405c1c73719f56`, correction parent `2f188cb6185061a43db36122aad68b5e253308d1`, cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`, TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. Reviewed original TASK-007/008, approved spec revision 20, R3 verdict/validation and R4 remediation, cumulative code and correction diff. No CodeGraph index exists. No current sibling discovery report was read.

Authorities inspected: AGENTS.md supplied in context; agent rules; spec-driven development; maintainer style; Wyrd design/doctrine; Bifrost design; security trust-boundary rules; routed DataFusion, OLAP, Iceberg, analytical reliability, testing and errors references. Approved spec/current user decisions govern conflicting historical text. FIND-007-3, Postgres tenant columns, established public error and no-cap decision remain excluded from findings.

Static only: no cargo, nextest, mise, build, runtime test, commit or source modification. Both permitted static whitespace commands (`git diff --check a7582db58 HEAD`, `git diff --check 2f188cb61 HEAD`) returned clean; concluding HEAD remained the candidate. Runtime results below are supplied evidence, not independent execution.

## Producer-to-consumer coverage

- Authenticated assignment → seal-key-scoped memtable collection (`memtable.rs:867–930`) and shard merge (`shards.rs:1147–1180`) → `LiveTailBatches` snapshot/generation exclusion (`tail_rpc.rs:573–594`) → session-partitioned MemorySourceConfig or shared HotParquetExec (`follower.rs:778–869`). No route-derived partition count or second staged decoder remains.
- Table binding → Scribe ArtifactPlan/ClaimAssembler footer (`parquet_writer.rs:603`, `claim_assembly.rs`, `staging_runtime.rs:714`) and Forge rewrite properties (`forge/managed/policy.rs:255–274`, `parquet/writer_properties.rs:119–128`) → shared footer proof. Footer uniqueness/missing/foreign checks are owned by `parquet/footer.rs:49–56,149–165`.
- Published metadata miss/hit → `PublishedFooterLoader::load` (`exec.rs:995–1058`) → proof before returned metadata; hot/staged cached metadata → `tenant_proven_reader_metadata` (`1141–1150`, `2959–2964`) → proof before row-group/page selection or decode. COUNT(*) does not bypass metadata proof.
- Staged source lease → `LiveTailBatches::into_parts` → HotParquetExec Arc lease → each hot stream capture (`exec.rs:2895–2905`) → drop releases registry leases (`hot_source.rs:259–264`). Metadata cancel-on-drop guard and native follower stream remain attached to their owner. Publication retirement waits on leases rather than replacing the scan authority.
- Lazy footer failure → Scribe stream classifier (`peer_service.rs:690–700`) → Aborted dispatch status (`648–658`) → remote `execute_candidate` stream conversion (`dispatcher.rs:1505–1515,1813–1823`) → typed `live_error` (`live.rs:639–648`) → leader first-refusal audit (`oracle/mod.rs:2484–2520`) and terminal mapping. TenantInvariant is excluded from availability loss (`live.rs:625–633`), so zero-row schema-only opening cannot degrade the refusal.
- Connection cancellation → StoppingIo read/write/flush token poll → connection driver error → owned response generator/native stream dropped. Connected returns exactly TcpConnectInfo (`wyrd-tonic/server/mod.rs:373–380`), preserving tonic TLS's `TlsConnectInfo<TcpConnectInfo>` consumed by private admission (`grpc/mod.rs:169–180`). The public listener still uses the existing graceful server (`app/server.rs:657–663`).
- Boot root preparation resolves once before lock/child derivation (`boot/data_root.rs:105–123`), maintaining local staged path identity. Cumulative digest v6 drops only removed caps while keeping tenant, table, writer, projection, predicate and reader authority; proto tags/names 7/8 remain reserved. ACK, WAL, publication and Forge settlement owners do not move into the connection wrapper.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK-007 R1/REQ-014: shared staged Parquet pruning; deleted bespoke decoder | follower live_leaf → HotParquetExec; staged_tail deleted | Existing staged pruning focused tests and supplied scoped module results | PASS |
| TASK-007 R2/R3: native memory source/filter and session partitions | follower.rs:790–826,854–869; resources follower execution; LiveScribeExec partition routing | Existing projection/filter and execution-shape tests; supplied Oracle journey lane | PASS |
| TASK-007 R4: native local frames, unchanged remote accounting/footer | ScribeFragmentExecutor native tally/completion, only peer adapter encodes; LiveFrameDecoder terminal/count checks | Existing native-frame/footer tests and supplied Oracle journeys | PASS |
| TASK-007 R5/INV-004: local Scribe scan, grant ownership | Local follower owns staged FileIO, memory pool and leases; leader only dispatches | Existing staged lease/drop tests; supplied journeys | PASS |
| TASK-008 R1/R4/R5: row tenant field/filter/tripwire removed, memory bound by seal | Managed columns/writes/codecs removed; memtable collector selects seal tenant/table/range | Existing schema/writer/follower tests and supplied prior codegen/module evidence | PASS |
| TASK-008 R2: authenticated footer through Scribe/Forge producers | Artifact/claim binding tenant; Forge config receives binding tenant | Footer identity tests; supplied promotion/rewrite/recovery journeys | PASS |
| TASK-008 R3/R6/REQ-015: prove fresh/cached metadata before decode, no fallback | Shared footer verifier; published and hot/staged metadata owners invoke it | Existing missing/foreign/duplicate negatives, distributed COUNT(*) proof; supplied 42/42 journeys | PASS |
| Remote refusal preserves class/audit; FIND-007-9 | Aborted now TenantInvariant at stream conversion; same typed leader consumer | New `remote_staged_footer_refusal_fails_closed` tests staged-only zero-row failure and one tenant_file event; supplied RED/GREEN | PASS/CLOSED |
| FIND-007-10: window-blocked Scribe shutdown releases fragments | Accepted peer IO fails independently of response-body polling; generator ownership drops with connection | New blocked-window journey uses 2M rows, producer-stall observation, graceful stop and release checks; supplied RED deadline failure/GREEN clean stop; pending-producer journey retained | PASS for Scribe correction |
| Preserve Oracle-only peer/Analytical lifecycle | Same wrapper is unconditionally used for every peer listener | Supplied peer lane does not establish preservation of Oracle-only graceful private streams | FAIL — INVARIANT-R4-001 |
| mTLS connect info/public serving/normal backpressure | Exact TcpConnectInfo delegation; unchanged public call; no production pump/queue | Source and supplied security/peer lane | PASS |
| FIND-007-8: cap-free authority matches implementation | bifrost-design.md:309–318 describes shallow source refs and execution pool | Supplied docs:check; static comparison to cut/collector/digest | PASS/CLOSED |
| FIND-007-7: evidence-only whitespace correction | TASK-006 blank-line correction | Both base-to-candidate diff checks independently clean | PASS/CLOSED |
| Non-goals/durable fences/replay retained | Wrapper owns only transport IO; durable publication and role settlement stay unchanged | Static owner/caller tracing; prior supplied durability tests | PASS |
| AC-016 benchmark obligation | Benchmark remains caller-owned; task says do not run capacity | Historical/current attributed TASK-006 evidence only | Outside this confirmation's execution/completion claim |

## Proposed finding

### INVARIANT-R4-001 — DRIFT/REGRESSION: forced closure also reaches Oracle-only private Analytical connections

**Violated obligation:** R4's FIND-007-10 correction requires preserving “Oracle-only peer/Analytical lifecycle behavior.” R3 independent validation narrowed the correction to private fragment-serving connections, explicitly excluded broader Oracle lifecycle changes, and stated Oracle-only peer/Analytical lifecycle need not acquire force-close semantics. This is separate from the approved public-listener preservation, which the candidate satisfies.

**Location:** `crates/wyrd/wyrd-server/src/app/server.rs:665–678`, especially unconditional `serve_peer_grpc_with_listener` at line 678; new force-close owner `crates/wyrd/wyrd-tonic/src/server/mod.rs:267–278,304–315`.

**Reachability/evidence:** `build_peer_grpc` mounts a private listener whenever either ingest or query capability exists (`grpc/mod.rs:405–420`). Oracle-only nodes therefore reach this call despite owning no Scribe fragment service. That listener mounts upstream Analytical GraphWorkerServices (`grpc/mod.rs:432–455`), lifecycle operations, and OraclePeerGrpc query forwarding (`peer_service.rs:612–641`). On shutdown the accepted socket wrapper returns ConnectionAborted immediately for all these connections. Before this correction the same Oracle-only listener used `serve_grpc_with_listener`, which invokes tonic's graceful incoming shutdown and permits outstanding peer streams to settle within the common drain budget. The new wrapper has no Scribe/fragment capability restriction.

**Observable consequence:** a stopping Oracle-only peer now resets an open private Analytical/forward-query response at transport IO instead of preserving its prior graceful connection lifecycle. A remote consumer can receive transport failure before its previous stream/terminal/protocol settlement. Role-owned cleanup still runs; this finding does not allege tenant leakage or acknowledged-data loss. The fact that stopping-process faults can ultimately fail a query does not establish authorization to broaden the explicitly preserved connection semantics.

**Smallest correction boundary:** retain cancellation-aware accepted IO on listeners whose existing role composition serves Scribe fragments (including mixed-role listeners sharing that boundary). Reuse the existing Bifrost Scribe capability/selected-role knowledge at `BoundServer` composition to keep the prior graceful serving owner for Oracle-only peers. Do not weaken token cancellation for blocked Scribe readers, move durable settlement, add per-stream pumps, or alter public listeners.

**Focused closure proof:** retain the new Scribe blocked-window RED/GREEN journey; add or reuse a real authenticated Oracle-only peer lifecycle proof that opens private Analytical work, requests graceful shutdown, and demonstrates preserved connection/settlement behavior within the existing budget. Verify both role-composition branches statically and retain existing mTLS/lifecycle tests. Independent validator should reconcile this proposal with the preserved-boundary authority; it must not infer a data-loss defect from the transport reset alone.

## Review conclusion

FIND-007-7, FIND-007-8 and FIND-007-9 are independently closed. The Scribe-specific root cause of FIND-007-10 is corrected at accepted IO rather than with another body guard, and resource/drop propagation is coherent. One proposed finding remains concerning its unconditional application to Oracle-only peers. No other acceptance or invariant defect was found in the cumulative task scope.
