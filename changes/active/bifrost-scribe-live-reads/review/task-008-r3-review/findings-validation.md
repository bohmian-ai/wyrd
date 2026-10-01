# Independent structured Ponytail validation

Recommendation: **FIX_REQUIRED**. **FIND-007-4 is CLOSED.** The deduplicated retained ledger is FIND-007-7 (new whitespace recurrence), FIND-007-8 (stale cap authority), FIND-007-9 (remote streamed tenant refusal), and FIND-007-10 (backpressured peer shutdown).

## Immutable subject, inputs, and limits

Candidate `2f188cb6185061a43db36122aad68b5e253308d1`; correction parent `9c3d7ecb982435919924dfa8e6930352b27a9b7e`; cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`; TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. HEAD was independently re-read and remains the candidate. There is no CodeGraph index.

This fresh validator received subject.md, cumulative.diff, final.diff, verification.md, all eight discovery reports, claim-comparison.md, and followup-review.md, without an intended verdict. Prior verdict/R3 remediation, original TASK-007/008, TASK-006 diagnosis, approved spec revision 20, AGENTS.md, agent-rules, spec-driven-development, maintainer-style, and applicable Bifrost design/contracts govern the assessment. Findings were independently checked against current source and callers rather than accepted by agreement. Pinned tonic 0.14.6 and Hyper 1.10.1 source was read locally to establish the transport behavior.

Current user authority prevails: no live-read cap; FIND-007-3 unchanged; Postgres tenant columns excluded; `WYRD_VALA_500_QUERY_TENANT_INVARIANT` stands. No finding below reopens these decisions. No cargo, nextest, mise, build, test, benchmark, commit, or source edit was performed. Only this report is written. Runtime results are supplied evidence, not fresh execution; capacity qualification remains caller-owned.

## Proposal reconciliation

| Discovery/follow-up IDs | Independent decision | Stable ledger ID |
|---|---|---|
| RSTD-R3-1 | CONFIRMED: newly added blank line contains trailing spaces; both explicit committed-range checks fail | FIND-007-7, recurrence of the same obligation |
| BEH-R3-1, INV-R3-1, MNT-R3-1, RSTD-R3-2 | CONFIRMED and deduplicated: one explicit current-architecture cap promise contradicts the deletion | FIND-007-8 |
| BEH-R3-2, FUP-001 | CONFIRMED and deduplicated: streamed Aborted loses the tenant class at the shared transport conversion | FIND-007-9 |
| SYSTEM-001, C-LIFECYCLE-01, FUP-002 | CONFIRMED defect; REVISED correction boundary to explicitly preserve sibling public/Oracle lifecycles and connect info | FIND-007-10 |

Blanket tenancy-preservation claims in the invariant, tenancy and system PASS rows do not hold for the already-open remote Scribe stream. Their native/open and footer-before-decode observations are correct, but omit `stream_status_error`. Likewise, pending-producer cancellation passes without proving a flow-control-blocked body. The focused follow-up resolves both uncertainties, and direct dependency/source inspection confirms those distinctions independently.

Rejected extensions: generic “bounded snapshot” language alone is not proof of a deleted numerical cap; proto reservations and digest history must remain. Restoring any cap, replacing the entire stream classifier with the open classifier, adding footer guards at each consumer, an unbounded producer pump, or changing globally shared public gRPC shutdown are unnecessary or alter adjacent behavior. No rejected extension is retained as optional advice.

## FIND-007-4 — independently CLOSED

`catalog/bifrost_catalog.rs:10` imports `iceberg::Error as IcebergError`; `provider_error` at line 1801 now accepts that bare name. Its complete body still wraps the identical Iceberg error in the existing DataFusion catalog variant. `assignment_schema`, `provider`, and `pinned_provider` use it at lines 1445, 1560 and 1600; none changed type or error behavior for R3.

`oracle/mod.rs:16` already imports `DataFusionError`; `is_tenant_refusal` at line 4330 now uses `&DataFusionError`. The complete predicate still checks the typed footer error and the stable tenant-invariant text. Its callers are `audit_tenant_refusal` at 2496, `map_datafusion_error` at 4313, and `query_stream::terminal_error_code` at 1283. R3 changes spelling only; the exact import/declaration obligation is satisfied. FIND-007-9 is a different upstream conversion defect, not grounds to keep this declaration finding open.

## Retained ledger

### FIND-007-7 — CONFIRMED / VIOLATION / whitespace recurrence

**Discovery:** RSTD-R3-1. **Location:** `changes/active/bifrost-scribe-live-reads/tasks/TASK-006-real-server-benchmark-and-remove-process-harness.md:82`.

**Obligation:** TASK-008 verification explicitly requires `git diff --check`; repository completion requires green relevant gates and truthful evidence. The earlier R2 normalization of earlier artifacts remains closed at those sites, but the same obligation recurs in new evidence.

**Independent evidence/consequence:** final.diff adds a blank line with two spaces. Both `git diff --check a7582db587c6170a290760f1741673125612b797 HEAD` and `git diff --check 9c3d7ecb982435919924dfa8e6930352b27a9b7e HEAD` independently report line 82 as trailing whitespace, contradicting line 101's clean-check claim. The committed candidate fails this static gate; no runtime defect is alleged.

**Minimum correction:** remove those spaces only, preserving diagnosis, results, attribution and surrounding content. Existing Git behavior supplies the check; no code, checker, harness, or benchmark is needed. Record the clean check against the corrected candidate.

**Focused closure proof:** both explicit cumulative and correction-parent Git whitespace checks exit zero, and the content-preserving one-line diff is inspected.

### FIND-007-8 — CONFIRMED / INCORRECT / deleted cap remains in active authority

**Discovery:** BEH-R3-1, INV-R3-1, MNT-R3-1, RSTD-R3-2. **Location:** `architecture/bifrost-design.md:311-312`.

**Obligation:** current user explicitly requests deleted-cap leftovers; repository architecture must reflect the lasting approved behavior. The live-fragment paragraph states retained bytes and batch count are enforced.

**Independent producer/consumer evidence:** `ScribeProviderCut` (`wyrd-spec/src/vala/api.rs:1953-1977`) carries only writer epoch and canonical partition endpoints. Leader `LiveScribeExec::fragment` constructs that cut; follower resolution passes no count/byte bound to `FetchLiveTailRequest`. `Memtable::collect_readable_batches` (`memtable.rs:899-945`) selects tenant/table/range under its existing locks, shallow-projects into a vector, and excludes durable immutable generations. `ScribeShardRuntime::snapshot` (`shards.rs:1147-1178`) extends merged vectors and retains mailbox/closure errors, not a snapshot count/byte ceiling. `open_live_batches` (`tail_rpc.rs:557-582`) retains generation deduplication and staged leases. The follower reuses the session memory pool for the staged scan; output totals reconcile a different invariant. None implements the promised snapshot caps.

**Observable consequence:** this task-related active authority promises the numerical enforcement intentionally deleted to fix 503s. It misdirects future maintenance/review toward restoring removed behavior. Exact deleted-name search across crates/architecture/docs independently finds only intentional proto reservations plus this concrete prose promise.

**Minimum correction:** edit this existing paragraph to remove numerical snapshot-cap enforcement and accurately describe shallow snapshot references and the existing query execution memory governance. Preserve authenticated projection/predicate/partition/epoch, deadline/cancellation, source leases and output terminal reconciliation. No runtime change or replacement resource policy is authorized or needed.

**Focused closure proof:** compare the paragraph with the cut/request/vector merger and follower pool; search for explicit current numerical live-snapshot promises while preserving proto reservations and explanatory digest history. This content correction needs no new runtime test.

### FIND-007-9 — CONFIRMED / INCORRECT / remote streamed tenant refusal becomes availability loss

**Discovery:** BEH-R3-2, FUP-001. **Correction location:** `crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs:1809-1815`, `stream_status_error`, called at 1501.

**Obligation:** spec REQ-004/015, INV-002 and AC-017; TASK-008 R3/R6. A missing, duplicate or foreign tenant footer must fail closed with the existing tenant-invariant outcome before file rows, including the leader's existing first-refusal security audit. It cannot become successful pre-row live omission.

**Independent reachable trace:**

1. `ScribeTailResolver::live_leaf` (`follower.rs:778-869`) accepts leased staged paths, reads size and builds the existing shared `HotParquetExec`. It does not read/prove their footers during construction. `PhysicalPlanFollower::execute` (`1431-1460`) returns a lazy execution stream.
2. `HotParquetExec::execute` (`exec.rs:2845-2872`) returns `hot_stream`. Its retained metadata fetch and `tenant_proven_reader_metadata` (`2883-2963`) occur when polled, before the Arrow decoder. A remote staged-only fragment with a wrong/missing/ambiguous footer therefore opens successfully and fails at first execution poll, with zero data rows.
3. `ScribeFragmentExecutor::execute` (`peer_service.rs:379-397`) maps that error through the full `scribe_stream_error` (`690-700`) to `DispatchError::TenantInvariant`. The gRPC adapter first yields schema (`572-576`), then `dispatch_status` (`648-658`) maps this error to Aborted. Tonic `codec/encode.rs:87-140` retains the status, flushing buffered frames before it if needed.
4. The complete authenticated remote consumer `TonicOraclePeerTransport::execute_candidate` (`dispatcher.rs:1470-1514`) maps errors after response opening through `stream_status_error`. Aborted falls through to Unavailable.
5. `LiveFragmentRead::into_stream` (`live.rs:563-576`) sees zero decoder rows and an allowed availability class, records a degraded source and skips that route. Schema does not increment the row count. The query can succeed degraded instead of refusing. Because no tenant error reaches `Oracle::audit_tenant_refusal` (`oracle/mod.rs:2484-2520`), its existing security classification cannot run. Even after earlier rows the wrong class loses the required tenant error/audit, although the query then fails generically.

**Siblings/root cause:** initial open errors already use `live_execution_status_error` → `execution_status_error` (`1778-1806`), reserving Aborted for TenantInvariant. The local native path preserves the enum; `live_error` (`639-647`) already maps TenantInvariant to the existing public error, and the existing audit/terminal consumers recognize it. One production stream-classifier caller serves remote candidates. The wrong state is manufactured at this conversion, not at footer production or the live availability policy.

**Minimum correction:** preserve Aborted as `DispatchError::TenantInvariant` in the existing shared `stream_status_error`. Reuse the established enum and private status convention. Retain its other classifications; do not substitute the entire open classifier, because that also changes unrelated streamed stale/source-loss/capacity semantics. No new public error, wire field, audit owner, footer check or downstream guard is needed. This consumer guard belongs at the trusted transport boundary that owns preserving the remote failure class.

**Focused closure proof:** a focused existing-owner classifier assertion for streamed Aborted plus a real-peer staged-only negative journey with no prior emitted rows: public query returns `WYRD_VALA_500_QUERY_TENANT_INVARIANT`, no file rows, and the existing leader refusal audit. Cover foreign/missing footer through the remote stream, not only initial open; retain genuine pre-row Unavailable degradation and after-row failure. Reuse PeerCluster/shared footer fixtures. The current open-status unit assertion and local/distributed published corruption journey do not exercise this conversion. Runtime execution is deferred by the current user restriction.

### FIND-007-10 — REVISED / INCORRECT / shutdown misses a flow-control-blocked peer body

**Discovery:** SYSTEM-001, C-LIFECYCLE-01, FUP-002. **Changed location:** `crates/wyrd/wyrd-server/src/oracle/peer_service.rs:572-601`. **Correction owner:** the existing private peer listener/connection lifecycle composed at `app/server.rs:665-695`, rather than another source/snapshot owner.

**Obligation:** the approved follow-on explicitly ends open fragment streams at server shutdown; the new field/method docs include paused or slow readers. Spec REQ-005 requires cancellation/disconnect to release source ownership, and Bifrost shutdown must preserve its existing process deadline and role settlement/replay behavior.

**Independent reachable trace:**

1. Scribe execution returns a native stream owning shallow rows or staged scan leases; `hot_stream` (`exec.rs:2898-2905`) captures its staged lease and metadata cancellation-on-drop guard. The peer adapter captures that stream inside a lazy generator, yields encoded batches, and races shutdown only during a subsequent generator poll.
2. An authenticated reader can stop consuming enough ordinary output to exhaust its HTTP/2 window. Tonic polls the generator only through encoded-body polling. Hyper 1.10.1 `proto/h2/mod.rs:134-185` waits on `body_tx.poll_capacity` while `buffered_data` exists, returning Pending before calling body `poll_frame`. A previously registered token wake does not bypass that gate; only resumed credit/reset lets the body select run.
3. `serve_grpc_with_listener` (`wyrd-tonic/src/server/mod.rs:234-244`) uses graceful incoming shutdown. Tonic 0.14.6 `transport/server/mod.rs:925-963` spawns detached connection tasks and calls `graceful_shutdown`; its outer server waits for watcher receivers to close (`868-876`). Neither action resets this still-active blocked response.
4. Supervision cancels the same token and waits against the common deadline (`app/supervise.rs:168-202`). Aborting the outer serving task does not abort tonic's detached connections. `app/server.rs:860-888` takes Bifrost abort if transport drain used the deadline. Source ownership can persist until peer reads/reset/query deadline or process death; in-process server teardown can retain that connection after the outer task is aborted.

The scenario needs no malformed input, synthetic load or cap restoration. Normal downstream backpressure on valid fragment output is sufficient. This is an incomplete new shutdown correction, not a demand for additional steady-state availability. No tenant leak or acknowledged-data deletion is alleged. The three-one-row-batch journey (`wyrd-testing/tests/bifrost/oracle/peer_network/analytical.rs:1160-1163,1203-1218`) continues reading during loss and proves pending-source cancellation only.

**Selected minimal safe correction:** make already-stopping private fragment-serving connections terminate independently of response-body polling, by using the existing server token at their accepted-IO lifetime. Tonic already accepts `AsyncRead + AsyncWrite + Connected` incoming IO (`transport/server/mod.rs:1094-1113`); a cancellation-aware accepted IO owner can register the token in the connection's read/write driver and return a connection error on cancellation, causing the existing HTTP/2 connection and owned streams to end. Use this native mechanism, retain normal pull backpressure, and avoid a new producer pump, timeout, task queue, transport or protocol. An IO error rather than a graceful EOF must interrupt outstanding sends; a trailer cannot be guaranteed to reach a receiver refusing flow-control credit.

**Preserved boundaries / revision of follow-up:** do not change the globally shared public `serve_grpc_with_listener` shutdown semantics. Apply the new termination only at the private fragment-serving peer boundary; Oracle-only peer/Analytical lifecycle need not acquire new force-close semantics to repair Scribe fragments. Where services share that stopping fragment listener, preserve their existing request/authentication ownership and durable settlement; no healthy process or unrelated public listener is shut down. Keep readiness/admission removal before token cancellation, mTLS, typed assignments, query-scoped failure semantics, and Scribe/Forge durable recovery and drain order. Forward exactly the existing TCP connect info: tonic TLS derives `TlsConnectInfo<TcpConnectInfo>` from it, and `grpc/mod.rs:169-172` requires that extension for peer authorization. A wrapper returning another connect-info type would break authentication and is not an acceptable simplification.

This is a bounded serving-boundary implementation correction under the already-authorized stopping-server behavior. It requires no new concurrency/resource-ownership policy or persistent-data decision. A body-only select or producer pump cannot close the blocked transport drain; public listener-wide resets or broader Oracle lifecycle changes would add scope and are rejected.

**Focused closure proof:** reuse the existing authenticated real-peer lifecycle test owner. Produce enough fragment output to exhaust HTTP/2 credit; hold the receiver open without draining/resetting; then request server shutdown. Prove source/lease ownership releases and the peer serving task settles within its existing drain budget without resuming reads or waiting for the logical query deadline. Retain pending-producer, client-disconnect, unavailable-before-row and fatal-after-row cases; assert authentication still works with unchanged TLS connect info. No new benchmark/harness is needed. This check was not executed under static-only review.

## Remaining assessed scope and decision

Cap deletion leaves authenticated tenant/table/partition selection, generation deduplication, staged leases and query execution owners intact; old proto tags/names are reserved and the v6 domain distinguishes changed signed bytes. The absolute root correction sits at `BifrostDataRoot::prepare` before managed children and exclusive lock derivation; it retains filesystem identity, write probes, WAL/staging/recovery and Forge-only disposable cleanup. These required corrections have no independently retained additional defect.

The four retained issues have separate owning invariants. They cannot be collapsed into one downstream guard; existing Git, architecture paragraph, transport classifier and private accepted-connection lifecycle respectively supply their smallest correction boundaries. FIND-007-4 is closed independently. Supplied tests are credible for the paths they actually cover, but do not disprove FIND-007-9 or FIND-007-10. No absent runtime execution is itself raised as a blocker. **Validated verdict recommendation: FIX_REQUIRED — FIND-007-7, FIND-007-8, FIND-007-9, FIND-007-10.**
