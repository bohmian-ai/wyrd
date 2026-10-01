# Independent behavior review — TASK-008-R4

Result: **FAIL**. Four prior defects are corrected at their diagnosed owners; one new listener scope violation is proposed for independent validation.

## Subject and method

Candidate `ca99db0af5a0d898ef67834699405c1c73719f56`; R4 parent `2f188cb6185061a43db36122aad68b5e253308d1`; cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`; TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`.

Read shared subject, original TASK-007/TASK-008, spec revision 20, R3 verdict and R4 remediation, cumulative and correction differences, repository rules, design/doctrine, applicable Bifrost authority and surrounding owners/callers. No current sibling report was read. Static only: no cargo, nextest, mise, builds, tests, or commits. Read-only cumulative and correction `git diff --check` exited zero. Runtime results below are supplied evidence, not freshly executed proof.

Traced authenticated query → retained root → live dispatch → Scribe follower → memory/shared Parquet scan → native or peer frames → decoder/terminal/audit; authenticated write → seal binding → staged/assembled and Forge footer; supervision → readiness withdrawal → listener cancellation → role settlement. TASK-006 intersections include real-peer harness, absolute root, shutdown and supplied benchmark diagnosis; unrelated benchmark qualification excluded. FIND-007-3, Postgres columns, selected public error, no-cap decision and closed FIND-007-4 imports remain untouched.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-014/TASK-007 R1: shared staged/hot/published pruning/cache scan; custom staged decoder deleted | `follower.rs:778–848` creates local `HotParquetExec`; `exec.rs:2883–3000` caches metadata, selects row groups/pages and projected decode; staged_tail deleted | Supplied staged pruning, lease release, published cache and module results | PASS |
| In-memory DataFusion source with same signed pushdown | `follower.rs:788–802,850–865` creates MemorySourceConfig, union and common predicate filter | Supplied projection/filter and partition tests | PASS |
| Session partition count; Scribe-local IO; admitted query pool | `follower.rs:787,837–841`, `resources.rs:1267–1277`, `peer_service.rs:355–366` | Supplied follower shape/partition and live journeys | PASS |
| Native local Arrow avoids IPC/hash; native/wire terminal counts remain | Directory transport selects local executor; ScribeFragmentExecutor yields Batch/Complete; LiveFrameDecoder::accept validates native totals/fingerprint versus remote digest/footer | Supplied native completion/frame and peer-service tests | PASS |
| Stream/source/lease ownership, cancellation/drop release | LiveTailBatches::into_parts transfers lease; `follower.rs:842–845` attaches lease; `exec.rs:2896–2907` retains it in stream; `live.rs:548–560` races grant deadline/cancellation | Supplied staged drop/overlap, pending-source loss and blocked-window stop journeys | PASS for fragment listener |
| REQ-015/TASK-008 R1/R4/R5: row tenant/filter/tripwire/codec deleted; memory seal bound | Managed envelope/schema/write/projection diffs delete row tenant and old nodes; memtable selects exact tenant/table seal key before projection | Supplied schema absence, closure, role isolation and write/read journeys | PASS |
| Footer producer uses authenticated binding through assembly/Forge | BifrostFooterIdentity::new/key_values, ArtifactPlan.tenant, AssembleClaimRequest.tenant, rewrite_writer_properties and Forge callers | Supplied footer/promotion/rewrite/recovery tests | PASS |
| Footer proof before file rows, cache and COUNT(*) included; no compatibility | PublishedFooterLoader::load and hot_stream call shared verify_scanned_footer_tenant before decoding; footer_value refuses missing/duplicate/foreign tenant | Supplied footer/hot negatives and local/distributed COUNT(*) journey | PASS |
| FIND-007-9: lazy remote refusal retains fatal class and leader audit | scribe_stream_error → TenantInvariant → Aborted → new `dispatcher.rs:1815` mapping → fatal `live.rs:569–570,641–643`; Oracle::audit_tenant_refusal consumes first matching error once | New staged-only foreign-footer journey: zero rows, Failed, QueryTenantInvariant and one leader security event; supplied RED Degraded → GREEN; classifier unit and Oracle 42/42 | PASS/CLOSED |
| FIND-007-8: active authority matches cap-free collector/merger/query-pool | `bifrost-design.md:311–317` updates no batch/byte limit and preserves scope/epoch/deadline/cancellation/lifetime | Source; supplied docs:check | PASS/CLOSED |
| FIND-007-7: whitespace fixed without evidence changes | TASK-006 whitespace-only correction | Both static diff whitespace checks exit zero | PASS/CLOSED |
| FIND-007-10: blocked Scribe IO ends without body poll/client drain | StoppingIo registers cancellation waker and fails connection read/write/flush below TLS/H2; connection destruction drops fragment stream | New 2M-row undrained-client journey observes stalled producer, requires clean stop, zero producer holds, failed read and Oracle release; supplied RED deadline → GREEN; 42/42 and 11/11 | PASS for diagnosed Scribe path |
| Preserve mTLS connect info, authentication and public listener | Connected forwards exact TcpConnectInfo; pinned tonic TlsStream derives TlsConnectInfo<T::ConnectInfo>; admission reads same TlsConnectInfo<TcpConnectInfo>; public task retains original runner | Source/type path; supplied authenticated real-peer suites | PASS |
| Preserve Oracle-only peer/Analytical lifecycle expressly required by R4 | `app/server.rs:678` selects stopping IO for every peer-bearing role, including Oracle-only listener with no fragments | Supplied suites do not cover preserving an active Oracle-only graceful exchange; source shows changed semantics | FAIL — B-R4-1 |
| Absolute root/recovery preserved | BifrostDataRoot::prepare resolves before children/lock; staged resolver reads these absolute paths | Supplied relative-root RED/GREEN/recovery proof, unchanged in R4 | PASS |

## Proposed B-R4-1

**Classification:** VIOLATION / REGRESSION.

**Obligation:** R4 explicitly says “Keep this at the private fragment-serving boundary” and “Preserve public `serve_grpc_with_listener` semantics and Oracle-only peer/Analytical lifecycle behavior.” The acceptance table repeats Oracle-only sibling preservation.

**Location:** `crates/wyrd/wyrd-server/src/app/server.rs:678`, unconditional stopping runner for every peer listener; `crates/wyrd/wyrd-tonic/src/server/mod.rs:267–280,311–320`, accepted wrapping/cancellation error.

**Reachability:** `grpc/mod.rs:401–465` builds a private router for Oracle-only nodes serving query. With no Scribe, it mounts Oracle control, analytical exchange and lifecycle services; it cannot serve a Scribe fragment. It still reaches the unconditional new runner. `state.rs:1927–1937` calls Oracle `start_draining` (`640–645`), which only withdraws readiness. `app/supervise.rs:184–202` then cancels transport and waits for serving tasks. Oracle engine/role cancellation happens later in `shutdown_owner` (`state.rs:716–725`) after that transport drain. Previously tonic 0.14.6 server `868–883,925–966` sent graceful GOAWAY and waited for accepted connections. Now every accepted Oracle-only socket fails with ConnectionAborted on the next IO poll in that interval.

**Consequence:** An already admitted Oracle-only analytical exchange/control response which could finish inside the original drain budget is forcibly reset instead of receiving its existing graceful transport lifetime. The remote analytical query sees terminal transport loss; its existing policy forbids retry. This happens on a listener with no Scribe fragment and extends beyond the explicit correction boundary. It does not claim a stopping node must serve forever.

**Minimal correction:** Select cancellation-aware accepted IO only when the private listener owns a Scribe fragment capability; retain existing graceful serve_grpc_with_listener for Oracle-only listeners. Reuse existing server role/owner composition, with no protocol, timeout, producer pump or resource change. A shared listener with Scribe stays inside the permitted boundary and retains existing authentication/durable owners.

**Focused closure:** Source should route Oracle-only through the original runner and Scribe-containing listeners through stopping IO. Reuse the real-peer harness to preserve an in-flight Oracle-only graceful exchange during ordinary shutdown, and keep blocked-Scribe stop/release, mTLS/public isolation and pending-source cancellation journeys green.

## Prior closure and limits

FIND-007-7/-8/-9 are closed. FIND-007-10's diagnosed blocked-Scribe failure is closed; B-R4-1 proposes a separate introduced sibling-boundary violation. No other material finding proposed. Test-support production counter is feature-gated and uses the existing serialized harness; supplied RED/GREEN is credible but was not executed here. Whole benchmark acceptance remains caller-owned.

The proposed correction uses existing role composition and two existing runners, so it needs no abstraction or dependency. Final authority interpretation and validation remain with the independent follow-up/validator.
