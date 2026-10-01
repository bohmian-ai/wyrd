# Independent structured Ponytail validation

**Recommendation: FIX_REQUIRED.** FIND-007-7, FIND-007-8 and FIND-007-9 are closed. FIND-007-10 is **REVISED / retained**: its blocked-Scribe cause is corrected, but its expressly required Oracle-only preservation is not. Two new standards findings are retained: FIND-007-11 and FIND-007-12.

## Immutable subject and limits

Candidate `ca99db0af5a0d898ef67834699405c1c73719f56`; correction parent `2f188cb6185061a43db36122aad68b5e253308d1`; cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`; TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. HEAD was independently reread and remained this candidate. No CodeGraph index exists.

Inputs: subject, cumulative/correction diffs, both task reviewers, standards, maintainer, system, tenancy, data and concurrency reports, claim comparison and focused follow-up. Governing inputs: original TASK-007/008, approved specification revision 20, R3 findings-validation/verdict and TASK-008-R4, AGENTS.md, agent rules, spec-driven development, maintainer style, routed Rust guidance and applicable Bifrost/service authorities. The proposals below were checked against current source, full corrected bodies, callers, adjacent consumers and local tonic 0.14.6 source; reviewer agreement was not used as proof.

Strict static review: no cargo, nextest, mise, builds, runtime tests, benchmarks, commits or source edits. Only this report is written. Git identity, status and both explicit committed-range whitespace checks were read; both whitespace checks exit zero. Runtime outcomes are supplied implementation evidence, not independently reproduced execution. Capacity qualification remains caller-owned. FIND-007-3, Postgres tenant columns, the established public error and no-cap decision stand unchanged.

## Every proposal reconciled

| Discovery / follow-up IDs | Independent disposition | Deduplicated ID |
|---|---|---|
| B-R4-1, INVARIANT-R4-001, FUP-R4-001 | CONFIRMED scope failure; REVISED ledger treatment to retain the existing correction's preservation obligation | FIND-007-10 |
| RSTD-R4-1 | CONFIRMED: per-item Errors sections and ConnectInfo documentation are explicitly required, including private trait implementations | FIND-007-11 |
| RSTD-R4-2 | CONFIRMED as an evidence-record violation; no assertion that the tests were never executed | FIND-007-12 |

No additional proposal appears in the maintainer, system, tenancy, data or concurrency reports. Their empty ledgers do not invalidate a source-supported unique standards finding. No rejected proposal is carried as optional advice.

The focused follow-up was warranted by the Oracle-only scope conflict. Its distinction is independently supported: reliable settlement after connection loss does not preserve the previously graceful connection interval. System/concurrency PASS observations about durable ownership, absence of steady-state change and private/public separation remain valid. Their inference that applying the reset to every stopping private listener satisfies R4 is rejected, because R4 separately and expressly preserves Oracle-only peer/Analytical lifecycle semantics.

Maintainer-style guidance yields to AGENTS §16. Neither documentation inheritance from AsyncRead/AsyncWrite nor documentation on the enclosing owner creates an exception to the explicit every-item/every-fallible-method rule. The broader green Oracle lane supports behavior, but AGENTS §11 and spec-driven-development's Test command precision expressly require the focused command record for each named test.

## Prior findings and cumulative acceptance

| Obligation | Independently inspected evidence | Result |
|---|---|---|
| FIND-007-7: whitespace | Correction preserves TASK-006 content; `git diff --check a7582db58 HEAD` and `git diff --check 2f188cb61 HEAD` return zero | CLOSED |
| FIND-007-8: cap-free current authority | `architecture/bifrost-design.md:309–317` describes shallow source references, no byte/batch limit, execution-pool governance and completion/drop lifetime. Collector/cut/merger do not impose deleted caps | CLOSED |
| FIND-007-9: remote lazy tenant refusal | `dispatcher.rs:1501,1813–1823` preserves Aborted; Scribe producer uses TenantInvariant/Aborted; `live.rs:566–580,639–648` makes it fatal even before data rows. `oracle/mod.rs:2484–2520` retains first-refusal canonical audit ownership. New staged-only journey requires zero rows, Failed/QueryTenantInvariant and exactly one leader security event | CLOSED behavior; command-record issue separately retained |
| FIND-007-10: flow-control-blocked Scribe | StoppingIo polls owned cancellation before socket read/write/flush, independently of lazy body polling; connection error drops body captures. New undrained 2M-row journey waits for production to stall, stops Scribe before consuming, requires clean stop/zero producers, then failed read and Oracle release. Supplied RED deadline/GREEN clean stop matches source diagnosis | Original cause corrected; complete closure FAIL below |
| Shared staged/published scan and memory source | `follower.rs:778–869` constructs session-partitioned MemorySourceConfig and HotParquetExec with signed predicates; `exec.rs:2883–3000` reuses governed footer cache/pruning/projected decode. Staged custom decoder remains deleted | PASS within static/supplied proof limits |
| Per-file tenant proof and seal-scoped memory | `memtable.rs:867–935` selects authenticated seal tenant/table/range; `footer.rs:49–56,149–165` refuses missing/duplicate/foreign identity. `PublishedFooterLoader::load:995–1058` and `tenant_proven_reader_metadata:1141–1153` prove fresh/cached metadata before decoder construction. Scribe/assembly/Forge producers retain authenticated binding tenant | PASS |
| Source and lease lifetime | `tail_rpc.rs:572–595` excludes generations already in memtable; `follower.rs:842–845` attaches staged lease; `hot_stream` captures it and its metadata cancellation drop guard. `StagedSourceLease::drop:259–264` releases through its existing registry | PASS |
| mTLS / public semantics | StoppingIo's Connected forwards exactly TcpConnectInfo. Tonic TLS preserves `TlsConnectInfo<TcpConnectInfo>` required by `grpc/mod.rs:169–180`. Public task still calls ordinary `serve_grpc_with_listener` at `app/server.rs:662` | PASS |
| Oracle-only preservation | Query-only peer router reaches unconditional reset runner before role-owned cancellation/drain | FAIL: FIND-007-10 |
| Documentation and focused evidence completion | New IO items lack required per-item sections; two named closure tests lack exact recorded commands | FAIL: FIND-007-11/12 |

Native local Arrow, wire completion/count validation, absolute data-root preparation, authenticated assignments/v6 digest/proto reservations, query-owned release, publication/ACK/WAL/Forge settlement and approved non-goals retain their existing owners. No independently validated additional cumulative defect is retained. This confirmation does not certify unrelated TASK-006 benchmark completion.

## Retained ledger

### FIND-007-10 — REVISED / VIOLATION and REGRESSION / Oracle-only preservation remains incomplete

**Discovery sources:** B-R4-1, INVARIANT-R4-001, FUP-R4-001. **Location:** `crates/wyrd/wyrd-server/src/app/server.rs:678`; force-close mechanism at `crates/wyrd/wyrd-tonic/src/server/mod.rs:267–278,304–315`.

**Violated obligation:** R3's validated correction and TASK-008-R4 explicitly restrict termination to the private fragment-serving boundary and preserve Oracle-only peer/Analytical lifecycle behavior. R4 acceptance repeats that preservation. This is not a new promise that a stopping Oracle must complete every query or outlive its existing process deadline.

**Producer-to-consumer proof:** `build_peer_grpc` (`grpc/mod.rs:405–478`) creates a router for ingest OR query capability; Oracle-only nodes mount Analytical exchange/lifecycle and query forwarding while `execute_fragment` explicitly refuses Oracle-target fragments (`peer_service.rs:554–558`). Every such listener nevertheless selects the new runner at line 678. `begin_shutdown` (`state.rs:1927–1938`) closes new public admission and withdraws readiness; Oracle `start_draining:640–645` does not cancel accepted exchange work. Supervision then cancels transport (`app/supervise.rs:184–202`) and waits for serving tasks. StoppingIo returns ConnectionAborted on the next IO poll, resetting accepted Oracle-only streams at this phase.

The old runner retains ordinary socket IO. Tonic 0.14.6 `transport/server/mod.rs:868–883,925–967` sends its graceful signal, calls connection graceful_shutdown and continues driving existing connections while awaiting their watchers. Oracle role cancellation/engine shutdown occurs later (`state.rs:716–725,1988–1995`). An accepted `forward_query` response (`peer_service.rs:612–641`) has already passed admission and lacks the Scribe body's shutdown select; it therefore provides a concrete additional consumer of the lost graceful interval. Oracle-only exchanges are production-supported role-separated paths, not unused or test-only surfaces.

**Consequence:** an admitted Oracle-only exchange or forwarding response that previously could finish during ordinary bounded transport drain now receives premature connection failure. On the coordinator channel, ReplayBody retains an AnalyticalConnectionLease (`analytical_transport.rs:240–269,418–432`); body loss invokes existing `close_connection` (`analytical.rs:2017–2030,3296–3311`), which can begin Cancelled settlement when the final connection drops. Cleanup remains correctly owned. No tenant leakage, acknowledged-data loss or suppressed settlement is alleged.

**Stable-ID decision:** preserve FIND-007-10 because Oracle-only preservation was already part of that finding's selected correction and acceptance. Its Scribe root cause is fixed; its full correction is not accepted. Giving the same unfulfilled preservation obligation a fresh ID and declaring -10 fully closed would obscure the remediation chain.

**Smallest safe correction:** reuse existing role/capability composition at the private listener owner to select stopping accepted IO only for listeners that actually serve Scribe fragments, including mixed-role fragment listeners. Keep Oracle-only listeners on existing `serve_grpc_with_listener`. No new timeout, protocol, pump, resource owner, dependency or shutdown framework is needed. Do not add guards at Analytical lease consumers: they already enforce correct ownership and are downstream of the unintended reset. Preserve mixed-listener durable settlement/authentication and unchanged public serving.

**Focused closure proof:** inspect both listener-role branches. Reuse the real authenticated peer harness for an admitted Oracle-only operation that can finish within the existing transport drain interval and require its prior completion/lifecycle behavior during graceful shutdown. Preserve the undrained blocked-Scribe clean-stop/release journey, pending-source/client-drop proof, mTLS/public separation and Analytical loss-settlement tests. Record exact focused commands/results when execution is authorized. Indefinitely blocked Oracle work need not gain unlimited drain.

### FIND-007-11 — CONFIRMED / VIOLATION / mandatory Rust item documentation missing

**Discovery source:** RSTD-R4-1. **Locations:** `crates/wyrd/wyrd-tonic/src/server/mod.rs:321,334,345,361,368,374`: poll_read, poll_write, poll_write_vectored, poll_flush, poll_shutdown and ConnectInfo.

**Obligation and evidence:** AGENTS §16 explicitly includes associated types and private items; every fallible method must include `# Errors`. Agent-rules makes missing/incomplete touched-item rustdoc a hard blocker; Rust documentation guidance repeats this. All five methods return `Poll<io::Result<_>>`, so they are fallible. The first four invoke poll_running and may produce ConnectionAborted or forwarded TCP errors. poll_shutdown delegates the fallible socket shutdown even after cancellation. Each has only a one-line summary, with no Errors section. ConnectInfo has no rustdoc. Full implementations and their sole production owner/caller were inspected; the surrounding type and poll_running docs do not meet per-item requirements.

**Consequence:** this changed transport boundary fails the explicit documentation completion rule and omits the required operation-level error/cancellation contract. This is a standards defect, not an inferred runtime failure or a preference for longer prose.

**Smallest correction:** document the five methods' actual error conditions and the associated connect-info type, preserving runtime code. Explain that read/write/flush refuse cancelled IO whereas write-half shutdown remains delegated; ConnectInfo remains the TCP identity tonic wraps with TLS certificates. Do not create an abstraction or checker, duplicate trait prose unnecessarily or alter untargeted items.

**Focused closure proof:** source inspection of these six items against §16 plus applicable format/documentation checks after authorization. No runtime test is warranted for this documentation-only correction.

### FIND-007-12 — CONFIRMED / VIOLATION / exact focused proof record incomplete

**Discovery source:** RSTD-R4-2. **Location:** `changes/active/bifrost-scribe-live-reads/review/task-008-r3-review/TASK-008-R4-preserve-remote-refusals-and-close-peer-shutdown.md:92–104`.

**Obligation and evidence:** AGENTS §11 and spec-driven-development's Test command precision require every named test in a task artifact/report to include and run its exact focused command, with package/target/environment wrapper. R4 names `open_stream_status_preserves_tenant_refusal` and `distributed::remote_staged_footer_refusal_fails_closed` with RED/GREEN claims. Its Commands block lists broad lanes and a focused expression only for the two Scribe release/stop journeys. Neither missing command is recorded elsewhere in the supplied R3 packet. The actual source names are `oracle::dispatcher::tests::open_stream_status_preserves_tenant_refusal` (vala-bifrost-redux lib) and `distributed::remote_staged_footer_refusal_fails_closed` (wyrd-testing oracle target).

**Consequence:** the task record cannot reconstruct its named classifier and staged-refusal focused runs as required. This does not prove those tests were unrun; broader supplied 42/42 remains credible behavior evidence.

**Smallest correction:** append the actual exact commands and selected-test results with attribution. If the focused runs did not occur, execute them only when separately authorized, then record them. Reuse existing nextest/source target and Postgres wrapper; do not change tests, broaden lanes, invent historical results or rerun capacity. The unit command belongs to `-p vala-bifrost-redux --lib` with the exact dispatcher selector and appropriate existing feature set; the journey belongs to `-p wyrd-testing --test oracle -P journey --run-ignored=all`, exact distributed selector, and repository-managed Postgres/migrations setup. The existing task evidence templates and owning mise lane define those recipes; no new harness is needed.

**Focused closure proof:** recorded exact expressions match current source/targets, include required setup, select the intended cases and carry observed results. Static review itself must not execute the absent proof.

## Decision

Validated ledger: **FIND-007-10 (REVISED), FIND-007-11 (CONFIRMED), FIND-007-12 (CONFIRMED)**. Each correction reuses an existing owner or mechanism and stays within approved behavior; no new product, public API, persistent-data, security or concurrency decision is required. **FIX_REQUIRED** is supported without reopening standing maintainer decisions or treating static-only review as a runtime gate failure.
