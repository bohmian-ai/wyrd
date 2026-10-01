---
id: TASK-008-R4
title: Preserve streamed tenant refusals and terminate stopping fragment connections
kind: remediation
status: ready
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 20
requirements: [REQ-004, REQ-005, REQ-014, REQ-015]
acceptance: [AC-016, AC-017]
parent_task: TASK-008
remediates: [FIND-007-7, FIND-007-8, FIND-007-9, FIND-007-10]
---

# TASK-008-R4

Route directly to `$wyrd-implement`. Apply the four independently validated bounded corrections; preserve the approved cap deletion and surrounding lifecycle behavior.

## Inputs and authority

Repository: the current Wyrd worktree. Approved spec: `changes/active/bifrost-scribe-live-reads/spec.md`, revision 20. Original tasks: `tasks/TASK-007-one-parquet-scan-for-live-reads.md` and `tasks/TASK-008-tenant-proven-per-file.md` under that change packet. Follow-on evidence: `tasks/TASK-006-real-server-benchmark-and-remove-process-harness.md`, “Benchmark failure diagnosis (2026-09-30)”.

Reviewed candidate: `2f188cb6185061a43db36122aad68b5e253308d1`; correction parent: `9c3d7ecb982435919924dfa8e6930352b27a9b7e`; cumulative TASK-007 base: `a7582db587c6170a290760f1741673125612b797`; TASK-008 base: `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. Prior verdict/R3: sibling `../task-008-final-review/`. Current diagnosis and recommendations: [findings-validation.md](findings-validation.md), [followup-review.md](followup-review.md), [verdict.md](verdict.md). Candidate source, repository rules and current user decisions govern; evidence alone is not behavior authority.

FIND-007-4 is closed. FIND-007-3 stands unchanged. Postgres tenant columns remain excluded. The public error remains `WYRD_VALA_500_QUERY_TENANT_INVARIANT`. There is no live-read snapshot byte/batch cap. No capacity rerun is part of this remediation.

## Diagnosis and selected correction

### FIND-007-9: remote streamed tenant refusal becomes availability loss

REQ-015/TASK-008 R3/R6 requires missing/foreign/duplicate footer proof to fail the query closed before file rows with the existing tenant-invariant error and leader security audit. The proof owner correctly refuses, but the remote transport loses its class.

`ScribeTailResolver::live_leaf` (`oracle/follower.rs:778–869`) builds a staged `HotParquetExec` without reading footers. `PhysicalPlanFollower::execute` returns its lazy stream. `hot_stream` proves footer metadata when first polled (`oracle/exec.rs:2883–2963`), after the RPC can open and emit schema but before file rows. `ScribeFragmentExecutor` maps that failure through `scribe_stream_error` to TenantInvariant; `dispatch_status` sends Aborted (`server/oracle/peer_service.rs:379–397,648–658,690–700`). The already-open remote stream uses `TonicOraclePeerTransport::execute_candidate` → `stream_status_error` (`oracle/dispatcher.rs:1470–1514,1809–1815`), which defaults Aborted to Unavailable. `LiveFragmentRead` (`oracle/live.rs:563–576`) may then omit the route before rows and finish successful/degraded; the leader never receives the tenant class for its refusal audit. After prior rows it fails generically with the wrong class/audit. Schema arrival does not count as a data row.

The existing initial-open classifier already reserves Aborted for TenantInvariant, and local native execution preserves it. Existing footer negatives, peer producer classification and published distributed refusal tests omit the intervening streamed conversion.

Selected correction: preserve Aborted as existing `DispatchError::TenantInvariant` in the shared `stream_status_error`, leaving its other classifications intact. This transport owner manufactures the invalid class and is the correct trusted conversion boundary. Reuse the existing enum/status convention and existing live error/audit consumers. Do not replace the whole classifier with the initial-open classifier, change legitimate availability degradation, duplicate footer checks, or add downstream guards. No protocol or public-error change is needed.

### FIND-007-10: shutdown cannot reach a flow-control-blocked response body

The authorized follow-on is that stopping the server ends open fragments, including slow/paused readers, while preserving role settlement and replay evidence. The new token race at `server/oracle/peer_service.rs:572–601` runs only when the lazy response body is polled.

An authenticated remote reader can stop draining valid output until its HTTP/2 window fills. Pinned Hyper 1.10.1 `PipeToSendStream::poll` waits on buffered-data send capacity before polling the body (`proto/h2/mod.rs:134–185`). Token wakeup therefore cannot run the body select or drop its captured native fragment/lease. Pinned tonic 0.14.6 gracefully shuts down detached connections and waits for them (`transport/server/mod.rs:868–876,925–963`); aborting the outer listener task does not abort those connections. Wyrd uses that graceful server at `wyrd-tonic/src/server/mod.rs:234–244`, composed by `app/server.rs:665–695`. Common shutdown waits to its deadline (`app/supervise.rs:168–202`), potentially consuming the role-drain budget and selecting Bifrost abort (`app/server.rs:860–888`). Durable evidence survives, but this does not fulfill clean fragment cancellation. The recorded lost-Scribe journey has three small batches and keeps reading; it proves pending-source cancellation, not transport window exhaustion.

Selected correction: use the existing shutdown token at the accepted-IO lifetime of the private fragment-serving listener, independently of response-body polling. Tonic already accepts `AsyncRead + AsyncWrite + Connected` incoming IO. Cancellation-aware accepted IO can wake/fail the connection read/write driver on shutdown so the existing connection drops its owned response streams and fragment resources. A connection error/reset must interrupt outstanding sends; graceful EOF or a body-only select can leave them blocked. An Unavailable trailer cannot be guaranteed to reach a receiver withholding credit; connection termination uses the existing availability outcome.

Keep this at the private fragment-serving boundary. Preserve public `serve_grpc_with_listener` semantics and Oracle-only peer/Analytical lifecycle behavior; do not force-close unrelated listeners or healthy processes. For services sharing the already-stopping fragment listener, preserve existing request/authentication and durable-settlement ownership. Forward exactly existing TCP connect info so tonic still supplies `TlsConnectInfo<TcpConnectInfo>` required by peer authorization (`grpc/mod.rs:169–172`). Preserve readiness/admission removal before cancellation, normal pull backpressure, query terminal policy, Scribe/Forge replay and settlement. No new producer pump, timeout, unbounded task queue, alternate snapshot owner or transport protocol. This corrects an authorized stopping-server boundary, not steady-state concurrency policy.

### FIND-007-8: active architecture still promises deleted caps

`architecture/bifrost-design.md:311–312` says live fragments enforce retained bytes and batch count. Current cut/request/vector collection and shard merge have neither cap; output totals and query execution memory are different invariants. The current authority contradicts the approved implementation and can lead maintainers to restore the diagnosed 503 cause.

Selected correction: update this existing paragraph to describe shallow source references without numerical snapshot caps and existing query execution memory governance. Preserve authenticated scope/projection/predicate/partition/epoch, deadline/cancellation, staged leases and terminal validation. Retain intentional proto reservations and digest history; generic source/range/lifetime bounds are not automatically wrong. Do not add resource policy or a replacement cap.

### FIND-007-7: new evidence reintroduces whitespace gate failure

`tasks/TASK-006-real-server-benchmark-and-remove-process-harness.md:82` is a newly added blank line containing two spaces. Both cumulative and correction-parent `git diff --check` fail despite the new section's clean-check claim. Earlier R2 sites remain closed; this is recurrence of the same completion obligation.

Selected correction: remove only those spaces, preserving all diagnostic content/results/attribution, and record a clean check for the corrected candidate. No runtime test or benchmark is needed.

## Acceptance and focused proof

| Finding | Required observable closure |
|---|---|
| FIND-007-9 | A streamed Aborted retains TenantInvariant; remote staged-only missing/foreign footer with zero prior data rows yields the existing public tenant-invariant error and leader refusal audit, never successful degradation. Genuine pre-row Unavailable still degrades; after-row unavailable still fails. |
| FIND-007-10 | Hold an authenticated fragment response open without draining/resetting after enough ordinary output exhausts HTTP/2 credit. Request server shutdown; source/lease ownership releases and peer serving settles within its existing drain budget, without resuming reads or waiting for the query deadline. Existing mTLS/connect-info authentication, pending-producer cancellation, disconnect and terminal behavior remain valid; public and Oracle-only sibling lifecycle semantics stay unchanged. |
| FIND-007-8 | Existing live-tail authority matches cap-free request/collector/merger and query-pool ownership, preserving remaining authority/lifetime guarantees. |
| FIND-007-7 | Content-preserving correction; explicit original-base and remediation-base whitespace checks both exit zero. |

For runtime changes, use ordered RED/GREEN/REFACTOR scenarios: streamed-status classification; remote staged-only footer refusal through real peer sockets; flow-control-blocked shutdown; then preserved sibling/authentication/availability paths. Reuse existing in-module classifier tests and `wyrd-testing` real PeerCluster lifecycle/footer owners. Do not add a new harness or synthetic host load. New exact test names are implementer-owned; record and execute each exact nextest expression with package/target/features and the repository-managed Postgres wrapper where required. A test selecting zero cases is not proof. Read tracing for any failure before diagnosis.

Current authorization is **static only**: no cargo, nextest, mise, builds, tests, benchmark or commits while this host is occupied. This review grants no execution exception. The commands below describe future verification once execution is separately authorized; do not run them under the present restriction.

Static closure:

```sh
git diff --check a7582db587c6170a290760f1741673125612b797
git diff --check 2f188cb6185061a43db36122aad68b5e253308d1
```

After an independently authorized commit, use explicit base-to-candidate forms for the confirmation review. Inspect the content-only changes and preserve attribution; no commit is authorized here.

Future broader scoped verification: `mise run fmt`, `mise run lints`, `mise run test:bifrost:journey:oracle`, `mise run test:server:peer`, plus exact focused classifier/lifecycle tests. Run `mise run docs:check` for affected documentation and the relevant client-tier/unwrap boundaries if the chosen correction touches them. Run `mise run codegen:check` only if contract/generated sources change; none is required by the selected correction. No full capacity benchmark, new dependency/feature/checker, broad gate, or unrelated refactor is justified by these bounded findings.

## Preserved scope and handoff

Preserve closed FIND-007-4 imports, v6 signed authority, proto reservations, absolute data root, authenticated footer-before-decode proof, native/wire counts, shared pool/lease owners, ACK/WAL/publication fences, legitimate source-loss semantics, and first-refusal canonical audit. Do not change Postgres tenancy, FIND-007-3 accounting, the no-cap decision, public query contracts, compatibility behavior, or whole benchmark acceptance. If the selected private serving correction cannot preserve the named sibling boundaries without a new material decision, report that concrete conflict instead of broadening shutdown semantics silently.

A later independent review reassesses the complete cumulative candidate against the original tasks and this validated ledger. This task does not authorize merge, push, deploy or commit.

## Implementation evidence (2026-09-30)

Execution was authorized by the maintainer ("continue with recommendations and fixes").

| Finding | Implementation | Verification | Result |
|---|---|---|---|
| FIND-007-9 | `oracle/dispatcher.rs` `stream_status_error` maps `Aborted` to `TenantInvariant` | unit `open_stream_status_preserves_tenant_refusal` RED→GREEN; journey `distributed::remote_staged_footer_refusal_fails_closed` RED (`rows=0 Degraded None`) → GREEN | PASS |
| FIND-007-10 | `wyrd-tonic` `serve_peer_grpc_with_listener` wraps each accepted peer socket in `StoppingIo` (fails IO with `ConnectionAborted` once the shutdown token fires; forwards `TcpConnectInfo`); only the peer task in `app/server.rs` uses it, public listener unchanged | journey `peer_network::analytical::remote_live_window_blocked_scribe_stops_cleanly` (2M undrained rows, producer stalled, graceful Scribe stop): RED `Bifrost shutdown deadline elapsed before role drain` → GREEN 23.4s, producer released, read fails, Oracles released; `remote_live_scribe_drop_releases_query` still passes (12.2s) | PASS |
| FIND-007-8 | `architecture/bifrost-design.md` live-tail paragraph describes cap-free shallow snapshot and query-pool governance | `mise run docs:check` exit 0 | PASS |
| FIND-007-7 | whitespace-only lines removed from TASK-006 | `git diff --check a7582db58…` and `git diff --check 2f188cb61…` both exit 0 | PASS |

Test support: `scribe::tail_rpc::live_batches_produced_for_test` (test-support only) tells a stalled producer from one still yielding.

Commands (all exit 0): `mise run fmt`; `mise run lints`; `mise run docs:check`; `mise run test:bifrost:journey:oracle` (42/42); `mise run test:server:peer` (11/11); focused:
`scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=peer_network::analytical::remote_live_window_blocked_scribe_stops_cleanly) | test(=peer_network::analytical::remote_live_scribe_drop_releases_query)'`.

Behavior note: on peer-listener shutdown every in-flight peer connection is now cut rather than awaited; unacknowledged peer ingest is retried by its caller and no durable settlement moved.
